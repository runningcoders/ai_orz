use serde::{Deserialize, Serialize};

/// Task 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub org_id: String,
    pub assigned_to: Option<String>,
    pub status: String,
    pub priority: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Task {
    pub fn new(title: String, description: String, org_id: String, priority: i32) -> Self {
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
            title,
            description,
            org_id,
            assigned_to: None,
            status: "pending".to_string(),
            priority,
            created_at: now,
            updated_at: now,
        }
    }
}
