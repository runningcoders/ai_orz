use serde::{Deserialize, Serialize};

/// Agent 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Agent {
    pub fn new(name: String, role: String, capabilities: Vec<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            id: uuid_simple(),
            name,
            role,
            capabilities,
            status: "idle".to_string(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// 简单 UUID 生成（后续可用 uuid crate）
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}
