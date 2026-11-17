use crate::pkg::aop::Event;
use common::enums::EventTopic;
use common::enums::task::TaskStatus;
use serde::{Deserialize, Serialize};

/// 任务状态变更事件
///
/// 唯一构造点：`service/dal/task.rs::publish_status_changed`（`update()` 与
/// `update_status()` 共用），在 SQL UPDATE 成功后发布。订阅者：
/// - TaskEventConsumer：异步消费，对 Completed 触发交付分发与后继补发
///
/// ⚠️ 交付身份是**发布时快照**，不是让消费者反查：消费者在 AOP Async 队列里
/// 跑，动作落库与消费之间任务/项目行可能已被改；靠反查会把派生消息挂到
/// 别人的链上，或把结果送给已经换掉的归属用户。
///
/// 通过 AOP 异步发布，确保所有 status 变更都触发通知，无论调用方是 domain 还是 handler。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusChangedEvent {
    pub event_id: String,
    pub task_id: String,
    pub task_title: String,
    pub project_id: Option<String>,
    pub assignee_id: String,
    pub old_status: TaskStatus,
    pub new_status: TaskStatus,
    pub progress: i32,
    pub created_at: i64,
    /// 归属用户 ID：结果最终关联给谁（与 `root_user_id` 语义一致）
    ///
    /// ⚠️ `serde(default)` 是升级兼容底线：AOP 队列是纯内存的，滚动升级时
    /// 旧进程发布的事件可能还留在队列里；缺字段必须能反序列化成「无快照」，
    /// 由消费者退回反查路径，而不是整条事件报错重试到丢弃。
    #[serde(default)]
    pub root_user_id: String,
    /// 任务发起者 Agent ID：谁派发、谁对交付收尾负责；空串 = 无发起者
    #[serde(default)]
    pub initiator_agent_id: String,
    /// 触发任务诞生的消息 ID：派单 / 回执 / 收尾挂回这条链；空串 = 另起新链
    #[serde(default)]
    pub trigger_message_id: String,
}

impl TaskStatusChangedEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_id: &str,
        task_title: &str,
        project_id: Option<&str>,
        assignee_id: &str,
        old_status: TaskStatus,
        new_status: TaskStatus,
        progress: i32,
        root_user_id: &str,
        initiator_agent_id: &str,
        trigger_message_id: &str,
    ) -> Self {
        Self {
            event_id: common::id::TimeId::generate().to_string(),
            task_id: task_id.to_string(),
            task_title: task_title.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            assignee_id: assignee_id.to_string(),
            old_status,
            new_status,
            progress,
            created_at: common::constants::utils::current_timestamp_ms(),
            root_user_id: root_user_id.to_string(),
            initiator_agent_id: initiator_agent_id.to_string(),
            trigger_message_id: trigger_message_id.to_string(),
        }
    }
}

impl Event for TaskStatusChangedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::TaskStatusChanged
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.task_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 滚动升级时 AOP 内存队列里可能残留旧格式事件（不带交付快照）。
    /// 缺字段必须反序列化成「无快照」由消费者兼容回退，而不是整条事件
    /// 报错重试到丢弃 —— 那会让升级窗口内的任务完成通知静默消失。
    #[test]
    fn legacy_event_without_delivery_snapshot_deserializes() {
        let raw = serde_json::json!({
            "event_id": "evt-legacy",
            "task_id": "task-1",
            "task_title": "legacy task",
            "project_id": "proj-1",
            "assignee_id": "agent-1",
            "old_status": "InProgress",
            "new_status": "Completed",
            "progress": 100,
            "created_at": 1,
        });
        let event: TaskStatusChangedEvent =
            serde_json::from_value(raw).expect("旧格式事件必须可反序列化");
        assert_eq!(event.root_user_id, "");
        assert_eq!(event.initiator_agent_id, "");
        assert_eq!(event.trigger_message_id, "");
        assert_eq!(event.kind(), EventTopic::TaskStatusChanged);
        assert_eq!(event.order_key(), "task-1");
    }
}
