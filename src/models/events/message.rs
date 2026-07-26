use crate::pkg::aop::{Event, EventKind};
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
    fn kind(&self) -> EventKind {
        EventKind::new("message.created")
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
