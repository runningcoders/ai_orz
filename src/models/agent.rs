use serde::{Deserialize, Serialize};

/// Agent 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: String,  // JSON 字符串存储
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl AgentPo {
    pub fn new(name: String, role: String, capabilities: Vec<String>) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_id(),
            name,
            role,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()),
            status: "idle".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 获取 capabilities 为 Vec
    pub fn get_capabilities(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())
}
