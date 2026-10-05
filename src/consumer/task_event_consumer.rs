//! Task event consumer (AOP async)
//!
//! 订阅 `task.status_changed` 事件，对 `TaskStatus::Completed` 的状态变更做两件事：
//! 1. **后继就绪补发**：给依赖本任务且前置已全量就绪的后继任务补发 `TaskAssignment`
//! 2. 项目 Owner Agent 通知（`TaskDispatchNotification`），驱动 Agent Loop Engine
//!    的 Layer 2 补偿机制
//!
//! 设计要点：
//! - `ConsumeMode::Async`：异步消费，发送方（DAL 层 update_status）不阻塞
//! - 仅处理 `TaskStatus::Completed` 事件（其他状态变更暂不通知）
//! - 发送前去重：检查目标 Agent 是否已有 Pending 的同类型消息
//! - 消息内容使用 `build_task_dispatch_content` 构建意图指令
//! - 消息填充 `project_id` + `task_id` 上下文字段，MessageConsumer 自动补充上下文
//!
//! # 为什么必须在这里补发后继任务
//!
//! 项目管理里 Owner 会把整张 DAG **一次性全部分派**出去，后继任务的
//! TaskAssignment 往往先于它的前置任务完成而到达。`wake_gate_policy` 的
//! `dependency_unmet` 门闩会跳过这类唤醒，而 AOP **没有延迟重投能力**
//! （只有 Retry/Discard，见 `pkg/aop/core/consumer.rs`）—— 跳过即 ack 终结。
//! 若没有这里的补发，后继任务会永久停在 Pending 且**没有任何报错**：
//! DAG 的推进责任会从一条确定的消息，转嫁给「Owner Agent 记得重新派发」这个
//! 概率性行为上。门闩负责省资源，补发负责不丢活，两者必须成对存在。

use async_trait::async_trait;
use common::enums::message::{MessageRole, MessageType};
use common::enums::task::TaskStatus;
use common::enums::{AssigneeType, EventTopic};
use common::error::Result;
use std::collections::HashMap;

use crate::models::events::TaskStatusChangedEvent;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::{SendTaskAssignmentCommand, SendToAgentCommand};

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

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::TaskStatusChanged)]
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

        // 补齐组织上下文（系统触发链路 ctx 无组织绑定，见 mod.rs helper 说明）
        // 提前到两路投递之前：后继补发与 Owner 通知共用同一份身份与组织上下文
        let ctx =
            crate::consumer::enrich_org_from_project_user(&ctx, &project.po.root_user_id).await;

        // 【身份分层模型】触发器按「被触达事项的归属」选择发送方身份：
        // - 项目归属用户非空 → **以用户身份中继**（from_role=User）：任务是用户发起的，
        //   调度结论用户需要感知；Agent 的 Final 自动回复会回到该用户（消息链自然
        //   路由，无需白名单特判），也不涉及伪造——这是上下文中继。
        // - 无归属用户（如 A2A 项目）→ 万不得已落 System：Final 无人可投递自然丢弃。
        //   二者都不会把回复路由回 Agent 自身 → 无自唤醒循环。
        let (from_id, from_role) = if project.po.root_user_id.is_empty() {
            ("system", MessageRole::System)
        } else {
            (project.po.root_user_id.as_str(), MessageRole::User)
        };

        // ① 后继就绪补发：门闩（wake_gate_policy::dependency_unmet）跳过的
        // 「前置还没跑完」的任务，在这里于前置完成时重新投递。失败只记日志 ——
        // 补偿路径不应把 TaskStatusChanged 事件打回重试，那会连带阻塞 Owner 通知
        if let Err(e) =
            dispatch_ready_successors(&ctx, project_id, &event.task_id, from_id, from_role).await
        {
            log_warn!(
                &ctx,
                "task_dispatch",
                task_id = %event.task_id,
                project_id = %project_id,
                error = ?e,
                "后继就绪补发失败（不阻塞 Owner 通知）"
            );
        }

        // ② 项目 Owner Agent 通知（Layer 2 补偿，驱动 Owner 审视全局编排）
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
            from_id,
            from_role,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(project_id),
            task_id: Some(&event.task_id),
            reply_to_id: None,
            external_key: None,
            thread_id: None,
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

/// 依赖已就绪的后继任务自动补发
///
/// 这是 `wake_gate_policy::dependency_unmet` 的另一半：门闩把「前置还没跑完」的
/// TaskAssignment 跳过了，而 AOP **没有延迟重投**（只有 Retry/Discard），跳过
/// 即 ack 终结 —— 没有这里的补发，后继任务会永久停在 Pending 且不报错。
///
/// 幂等来自三道闸（缺一都可能重复派发）：
/// 1. 后继必须仍是 `Pending` —— 已被认领、已在跑、已完结的任务不重复派发
/// 2. 全部前置都必须是 `Completed`（含本次刚完成的这一个）
/// 3. 承接 Agent 上不能有未投递的同类型指派消息（沿用 Owner 通知的去重口径）
async fn dispatch_ready_successors(
    ctx: &RequestContext,
    project_id: &str,
    finished_task_id: &str,
    from_id: &str,
    from_role: MessageRole,
) -> Result<()> {
    let tasks = crate::service::domain::project::domain()
        .task_manage()
        .list_by_project(ctx.clone(), project_id)
        .await?;

    // id → Task 索引：项目内任务量可控，一次全量拉取胜过为 JSON 依赖列写反向 SQL
    let index: HashMap<&str, &Task> = tasks.iter().map(|t| (t.po.id.as_str(), t)).collect();

    for task in &tasks {
        let deps = task.po.get_dependencies();

        // 只关心「刚完成的这个任务」这条边上的直接后继
        if !deps.iter().any(|d| d == finished_task_id) {
            continue;
        }
        // 已被认领 / 已开始的任务不该被重复指派
        if task.po.status != TaskStatus::Pending {
            continue;
        }
        // 指派给人的任务走人的列表，不走消息
        if task.po.assignee_type != AssigneeType::Agent {
            continue;
        }
        // 全部前置必须真完成。索引查不到的依赖（脏数据 / 跨项目引用）判为未就绪：
        // 宁可不发，也不要在输入不齐时启动下游
        let all_ready = deps.iter().all(|d| {
            index
                .get(d.as_str())
                .is_some_and(|t| t.po.status == TaskStatus::Completed)
        });
        if !all_ready {
            continue;
        }

        let message_domain = crate::service::domain::message::domain();
        if message_domain
            .has_pending_message_for_agent(
                ctx.clone(),
                &task.po.assignee_id,
                MessageType::TaskAssignment,
            )
            .await?
        {
            log_debug!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                "该 Agent 已有未投递的任务指派，跳过补发"
            );
            continue;
        }

        let cmd = SendTaskAssignmentCommand {
            task_id: &task.po.id,
            task_title: &task.po.title,
            task_description: Some(task.po.description.as_str()),
            from_id,
            from_role,
            to_agent_id: &task.po.assignee_id,
            project_id: Some(project_id),
        };

        if let Err(e) = message_domain
            .delivery()
            .send_task_assignment(ctx.clone(), cmd)
            .await
        {
            log_warn!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                error = ?e,
                "前置依赖已就绪，但任务指派补发失败"
            );
        } else {
            log_info!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                "前置依赖已全部就绪，补发任务指派"
            );
        }
    }

    Ok(())
}
