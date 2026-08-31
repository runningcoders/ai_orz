//! Task event consumer (AOP async)
//!
//! 订阅 `task.status_changed` 事件，对 `TaskStatus::Completed` 的状态变更触发
//! 项目 Owner Agent 通知（TaskDispatchNotification），驱动 Agent Loop Engine
//! 的 Layer 2 补偿机制。
//!
//! 设计要点：
//! - `ConsumeMode::Async`：异步消费，发送方（DAL 层 update_status）不阻塞
//! - 仅处理 `TaskStatus::Completed` 事件（其他状态变更暂不通知）
//! - 发送前去重：检查 Owner Agent 是否已有 Pending 的 TaskDispatchNotification
//! - 消息内容使用 `build_task_dispatch_content` 构建意图指令
//! - 消息填充 `project_id` + `task_id` 上下文字段，MessageConsumer 自动补充上下文

use async_trait::async_trait;
use common::error::Result;

use crate::models::events::TaskStatusChangedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::message::SendToAgentCommand;
use common::enums::message::{MessageRole, MessageType};
use common::enums::task::TaskStatus;

pub struct TaskEventConsumer;

impl TaskEventConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskEventConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for TaskEventConsumer {
    fn name(&self) -> &str {
        "task_event"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("task.status_changed")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize TaskStatusChangedEvent: {}",
                e
            ))
        })?;

        // 仅处理任务完成事件
        if event.new_status != TaskStatus::Completed {
            return Ok(());
        }

        // 项目级通知：必须有 project_id 才能定位 Owner Agent
        let Some(project_id) = &event.project_id else {
            return Ok(());
        };

        // ctx 已由框架从事件 carrier 还原（保留 log_id 等链路标识）

        // 查询项目的 Owner Agent
        let project = crate::service::domain::project::domain()
            .project_manage()
            .get(ctx.clone(), project_id)
            .await?;

        let Some(project) = project else {
            return Ok(());
        };
        let Some(owner_agent_id) = &project.po.owner_agent_id else {
            return Ok(());
        };

        let message_domain = crate::service::domain::message::domain();

        // 合并去重：检查是否已有同 Agent 的 Pending TaskDispatchNotification
        let has_pending = message_domain
            .has_pending_message_for_agent(
                ctx.clone(),
                owner_agent_id,
                MessageType::TaskDispatchNotification,
            )
            .await?;

        if has_pending {
            log_debug!(
                &ctx,
                "task_dispatch",
                agent_id = %owner_agent_id,
                task_id = %event.task_id,
                "已有 Pending 的 TaskDispatch 消息，跳过本次通知"
            );
            return Ok(());
        }

        // 构建消息内容（意图指令嵌入消息本体）
        let content = crate::service::domain::message::builder::build_task_dispatch_content(
            &event.task_title,
            event.new_status,
            event.progress,
        );

        // 发送消息（填充 project_id + task_id，MessageConsumer 自动补充上下文）
        let cmd = SendToAgentCommand {
            from_id: "system",
            from_role: MessageRole::System,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(project_id),
            task_id: Some(&event.task_id),
            reply_to_id: None,
            attachment_ids: None,
            message_type: MessageType::TaskDispatchNotification,
        };

        let log_ctx = ctx.clone();
        if let Err(e) = message_domain.delivery().send_to_agent(ctx, cmd).await {
            log_warn!(
                &log_ctx,
                "task_dispatch",
                task_id = %event.task_id,
                project_id = %project_id,
                agent_id = %owner_agent_id,
                error = ?e,
                "发送任务调度通知失败"
            );
        }

        Ok(())
    }
}
