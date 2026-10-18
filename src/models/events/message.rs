use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCreatedEvent {
    pub message_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub from_id: String,
    pub from_role: i32,
    pub to_id: String,
    pub to_role: i32,
    pub message_type: i32,
    pub content: String,
    pub created_at: i64,
}

impl Event for MessageCreatedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::MessageCreated
    }

    fn id(&self) -> &str {
        &self.message_id
    }

    fn order_key(&self) -> &str {
        // 分层 order_key：接收者为 Agent 时按 agent_id 串行；否则按 task → project 降级
        //
        // 【Agent 接收者】用 to_id（agent_id）：
        // - Agent 的 busy 状态是全局的（不区分 task），同 agent 消息必须串行
        // - 避免不同 worker 并发取同 agent 消息后 try_set_busy 失败的重试开销
        // - 把串行点从"失败重试"提前到"队列层"，减少无效 IO
        //
        // 【非 Agent 接收者】（User/System）用 task_id → project_id：
        // - 无状态竞争，不需要 agent 维度串行
        // - 保持用户消息按 task 顺序投递（用户在 task 上下文中看到有序消息）
        // - 避免 project 维度串行导致同 project 跨 task 阻塞
        use common::enums::MessageRole;
        if self.to_role == MessageRole::Agent as i32 {
            &self.to_id
        } else {
            self.task_id
                .as_deref()
                .unwrap_or_else(|| self.project_id.as_deref().unwrap_or(""))
        }
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️⚠️ 这条不变量是整个「滞留重投不会重复投递」机制的基石，**不要删也不要改**：
    ///
    /// AOP 内存队列用 `event_id` 去重（`InMemoryEventQueue::do_enqueue` 入口判
    /// `events.contains_key(event_id)`），而定时重投任务每 5 分钟会把同一条消息再
    /// publish 一次。二者叠起来成立的唯一原因就是：`event_id == message_id`，
    /// 于是「同一条消息」在队列里最多一份事件。
    ///
    /// 一旦有人把 `id()` 改成随机 uuid（看似更"规范"），重投会立刻退化为真重复投递：
    /// 同一条消息被同个 Agent 并发消费两次。这种 bug 只在机器忙的时候出现，极难复现。
    #[test]
    fn event_id_is_the_message_id_redelivery_dedup_depends_on_it() {
        let event = MessageCreatedEvent {
            message_id: "msg-1".to_string(),
            project_id: Some("proj-1".to_string()),
            task_id: Some("task-1".to_string()),
            from_id: "u1".to_string(),
            from_role: 2,
            to_id: "a1".to_string(),
            to_role: 1,
            message_type: 10,
            content: "hi".to_string(),
            created_at: 1000,
        };

        assert_eq!(event.id(), "msg-1");
        assert_eq!(event.id(), event.message_id);
        assert!(matches!(event.kind(), EventTopic::MessageCreated));
    }

    /// 同一消息构造两次 ⇒ id 必须相同（幂等键的定义）
    #[test]
    fn same_message_yields_same_event_id_across_republishes() {
        let build = || MessageCreatedEvent {
            message_id: "msg-42".to_string(),
            project_id: None,
            task_id: None,
            from_id: "u1".to_string(),
            from_role: 2,
            to_id: "a1".to_string(),
            to_role: 1,
            message_type: 11,
            content: "followup".to_string(),
            created_at: 2000,
        };

        assert_eq!(build().id(), build().id());
    }
}
