use serde::{Deserialize, Serialize};

/// Message 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub content: String,
    pub created_at: i64,
}

impl Message {
    pub fn new(from_agent_id: String, to_agent_id: String, content: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            id: format!(
                "{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            from_agent_id,
            to_agent_id,
            content,
            created_at: now,
        }
    }
}
