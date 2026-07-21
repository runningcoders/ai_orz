use serde::{Serialize, Deserialize};
use crate::pkg::aop::{Event, EventKind};

pub const A2A_TASK_ID_TAG_PREFIX: &str = "a2a_task_id:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A2aUpdateSource {
    Callback,
    Polling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTaskUpdateEvent {
    pub event_id: String,
    pub local_task_id: String,
    pub remote_agent_id: String,
    pub remote_task_id: String,
    pub source: A2aUpdateSource,
    pub task_json: String,
    pub created_at: i64,
}

impl A2aTaskUpdateEvent {
    pub fn extract_a2a_task_id(tags: &[String]) -> Option<String> {
        tags.iter()
            .find(|t| t.starts_with(A2A_TASK_ID_TAG_PREFIX))
            .map(|t| t[A2A_TASK_ID_TAG_PREFIX.len()..].to_string())
    }

    pub fn make_a2a_task_tag(remote_task_id: &str) -> String {
        format!("{}{}", A2A_TASK_ID_TAG_PREFIX, remote_task_id)
    }
}

impl Event for A2aTaskUpdateEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("a2a.task.update")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.local_task_id
    }

    fn priority(&self) -> u8 {
        5
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
