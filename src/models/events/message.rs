use serde::{Serialize, Deserialize};
use crate::pkg::aop::{Event, EventKind};

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
        self.project_id.as_deref().unwrap_or("")
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}