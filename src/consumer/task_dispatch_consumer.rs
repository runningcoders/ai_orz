//! 任务派发消费者：把「任务已落库」这个动作转成 `TaskAssignment` 消息
//!
//! 交付责任制里的**派单一跳**。动作方（`create_task` handler）只负责把任务行
//! 写进 DB 并 `aop::publish(TaskDispatchedEvent)`，消息的类型、收件人、链锚点
//! 全在这里收口 —— 与 `task_event_consumer` 的「完成一跳」形成对称。
//!
//! ## 为什么是 Sync
//!
//! `ConsumeMode::Sync` 让 `aop::publish` **内联执行** `on_event`，事件不进内存
//! 队列。这不是性能取舍，是可靠性红线：Async 队列是纯内存的，进程重启即丢，
//! 而「任务已落库、派单消息却没发出去」是一个不会报错、没人会再来唤醒的
//! 死任务。现有的 `dispatch_ready_successors` 只补 DAG 后继、`scheduler` 的
//! 滞留重投白名单明确排除 `TaskAssignment`，都救不了首次派发。
//!
//! ⚠️ 若将来改回 Async，必须同时补一条「扫 `Pending` 且无 `TaskAssignment`
//! 消息的任务行」的补偿扫描，否则静默死任务窗口会原样回来。

use async_trait::async_trait;
use common::enums::EventTopic;
use common::error::Result;

use crate::models::events::TaskDispatchedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::{self, SendTaskAssignmentCommand};

pub struct TaskDispatchConsumer;

impl TaskDispatchConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskDispatchConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for TaskDispatchConsumer {
    fn name(&self) -> &str {
        "task_dispatch"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        // ⚠️ 不要 `.ordered()`：Sync 消费者声明顺序消费是谎言，注册期直接拒。
        vec![Subscription::new(EventTopic::TaskDispatched)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: TaskDispatchedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize TaskDispatchedEvent: {e}"
            ))
        })?;

        // 系统/工具链路的 carrier 可能缺组织，按归属用户补齐，避免消费侧
        // rebuild_context 还原出的 ctx 因缺组织而在工具调用里报 400。
        let ctx = crate::consumer::enrich_org_from_project_user(&ctx, &event.root_user_id).await;

        // 派发者身份：任务的发起者 Agent（真正派活的人）；管理面直建的任务
        // 没有 Agent 参与，退回用户本人；两者皆空才落 system。
        let (from_id, from_role) =
            crate::consumer::task_sender_identity(&event.initiator_agent_id, &event.root_user_id);

        let cmd = SendTaskAssignmentCommand {
            task_id: &event.task_id,
            task_title: &event.task_title,
            task_description: Some(event.task_description.as_str()),
            from_id,
            from_role,
            to_agent_id: &event.assignee_id,
            // 所属用户：与派发者正交，工具凭据 / 用户画像 / 交付终点都用它
            root_user_id: &event.root_user_id,
            project_id: event.project_id.as_deref(),
            // 派单是发起任务那条消息的衍生消息：继承链根，不另起新链。
            // 空锚点（用户直接建任务 / 存量任务）退回自身为链根。
            reply_to_id: (!event.trigger_message_id.is_empty())
                .then_some(event.trigger_message_id.as_str()),
        };

        match message::domain()
            .delivery()
            .send_task_assignment(ctx.clone(), cmd)
            .await
        {
            Ok(_) => {
                log_info!(
                    &ctx,
                    "task_dispatch",
                    task_id = %event.task_id,
                    agent_id = %event.assignee_id,
                    "任务派发动作已转成 TaskAssignment 消息"
                );
                Ok(())
            }
            Err(e) => {
                // Sync 下投递结论被框架忽略，但错误必须浮出来：派单失败不记账
                // 就等于制造一个永远不会有人来唤醒的任务。
                log_warn!(
                    &ctx,
                    "task_dispatch",
                    task_id = %event.task_id,
                    agent_id = %event.assignee_id,
                    error = ?e,
                    "任务已落库但派单消息发送失败"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use common::enums::MessageRole;

    use crate::consumer::task_sender_identity;

    /// 派发者三档：发起者 Agent 优先，管理面直建退回用户，两者皆空才落 system。
    /// ⚠️ 有发起者时**不得**用归属用户兜底 —— 那正是被废弃的「用户身份中继」，
    /// 会让执行者的 Final 越过发起者直接到用户。
    #[test]
    fn task_sender_identity_prefers_initiator_over_root_user() {
        assert_eq!(
            task_sender_identity("agent-initiator", "user-owner"),
            ("agent-initiator", MessageRole::Agent)
        );
        assert_eq!(
            task_sender_identity("", "user-owner"),
            ("user-owner", MessageRole::User)
        );
        assert_eq!(
            task_sender_identity("", ""),
            ("system", MessageRole::System)
        );
    }
}
