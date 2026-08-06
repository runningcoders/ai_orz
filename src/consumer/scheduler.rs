//! Cron 触发器消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 CRON_TRIGGER 事件。
//! 业务逻辑通过调用 domain 层完成（如 RuntimeAwakening.sleep_and_settle）。
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::handlers::hr::agent::settle_memory::load_and_settle;
use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use common::error::{Error, Result};

// ==================== 消费者实现 ====================

/// Cron 触发器消费者
///
/// 订阅 CRON_TRIGGER 事件，按 payload.action 分发到不同 domain 处理。
/// 作为 AOP 的 Sync 消费者，事件发布时直接调用 on_event。
pub struct CronTriggerConsumer;

impl Default for CronTriggerConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTriggerConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for CronTriggerConsumer {
    fn name(&self) -> &str {
        "cron_trigger"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("cron.trigger")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: CronTriggerEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize cron trigger event: {}", e))
        })?;

        sys_debug!(
            "received cron trigger event: {} (trigger_id: {}, action to be parsed)",
            event.id(),
            event.trigger_id
        );

        let payload: CronTriggerPayload = serde_json::from_str(&event.payload).map_err(|e| {
            Error::bad_request(format!(
                "invalid cron trigger payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "cron trigger fired: {} (trigger_id: {}, action: {})",
            event.trigger_name,
            event.trigger_id,
            payload.action
        );

        match payload.action.as_str() {
            "agent_rest" => {
                self.handle_agent_rest(&event, &payload.extra).await?;
            }
            "project_followup" => self.handle_project_followup(&payload.extra).await?,
            _ => {
                sys_warn!(
                    "unknown action '{}' for trigger {} (id: {})",
                    payload.action,
                    event.trigger_name,
                    event.trigger_id
                );
            }
        }

        Ok(())
    }
}

// ==================== 业务编排（调用 domain 层）====================

impl CronTriggerConsumer {
    /// agent_rest 动作：加载 Agent 并调用 sleep_and_settle 执行记忆沉淀
    ///
    /// 复用 settle_memory handler 的 load_and_settle 公共函数，保证与神经工具触发的
    /// 沉淀流程完全一致（查询短期记忆 → 拼装 prompt → 加载 Agent → 唤醒 Brain → sleep_and_settle）。
    async fn handle_agent_rest(&self, event: &CronTriggerEvent, extra: &Value) -> Result<()> {
        let payload: AgentRestPayload = serde_json::from_value(extra.clone()).map_err(|e| {
            Error::bad_request(format!(
                "invalid agent_rest payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "agent_rest action triggered by {} (trigger_id: {}, agent_id: {})",
            event.trigger_name,
            event.trigger_id,
            payload.agent_id
        );

        let ctx = RequestContext::new_system();
        let settle_limit = payload.settle_limit.unwrap_or(10);

        let settled_count = load_and_settle(ctx, &payload.agent_id, settle_limit).await?;

        sys_info!(
            "agent {} settled {} short-term memories to knowledge nodes",
            payload.agent_id,
            settled_count
        );

        Ok(())
    }

    /// project_followup 动作：对所有进行中且有 Owner Agent 的项目发送跟进通知
    ///
    /// 定时补偿场景（Agent Loop Engine 场景 3）：扫描所有 InProgress 且
    /// owner_agent_id 非空的项目，向 Owner Agent 发送 ProjectFollowupNotification
    /// 消息，驱动其检查项目进度并处理阻塞任务。
    /// 预检查 Agent 运行时状态：Busy/Resting 时跳过，避免无意义 nack 堆积。
    async fn handle_project_followup(&self, _extra: &Value) -> Result<()> {
        use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
        use crate::service::domain::message::SendToAgentCommand;
        use crate::service::domain::message::builder::build_project_followup_content;
        use crate::service::domain::message::domain as message_domain;
        use crate::service::domain::project::domain as project_domain;
        use common::enums::{MessageRole, MessageType};

        let ctx = RequestContext::new_system();

        // 1. 查询所有进行中且有 Owner Agent 的项目
        let projects = project_domain()
            .project_manage()
            .list_in_progress_with_owner(ctx.clone())
            .await?;

        for project in projects {
            let owner_agent_id = match &project.po.owner_agent_id {
                Some(id) => id,
                None => continue,
            };

            // 2. 预检查：Agent 必须空闲才发送，避免无意义 nack 堆积
            let state = AgentRuntimeStateManager::global().get_state(owner_agent_id);
            if state.is_unavailable() {
                sys_info!("Agent {} 当前 {:?}，跳过项目跟进", owner_agent_id, state);
                continue;
            }

            // 3. 构建消息内容（意图指令嵌入消息本体）
            let content = build_project_followup_content(&project.po.name);

            // 4. 发送消息（填充 project_id 上下文，MessageConsumer 会自动补充 project 信息）
            let cmd = SendToAgentCommand {
                from_id: "system",
                from_role: MessageRole::System,
                to_agent_id: owner_agent_id,
                content: &content,
                project_id: Some(&project.po.id),
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            };

            if let Err(e) = message_domain()
                .delivery()
                .send_to_agent(ctx.clone(), cmd)
                .await
            {
                sys_warn!("发送项目跟进消息失败: agent={}, err={}", owner_agent_id, e);
            }
        }

        Ok(())
    }
}

// ==================== 辅助类型 ====================

#[derive(Debug, Serialize, Deserialize)]
struct CronTriggerPayload {
    action: String,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentRestPayload {
    agent_id: String,
    settle_limit: Option<usize>,
}
