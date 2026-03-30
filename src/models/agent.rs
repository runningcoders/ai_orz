use serde::{Deserialize, Serialize};
use crate::pkg::constants::AgentPoStatus;

/// Agent 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: String,        // 长文本，JSON 字符串存储
    pub soul: String,                // 长文本，Agent 灵魂/性格描述
    pub status: AgentPoStatus,       // 软删除状态
    pub created_by: String,           // 创建者
    pub modified_by: String,          // 修改者
    pub created_at: i64,
    pub updated_at: i64,
}

impl AgentPo {
    pub fn new(
        name: String,
        role: String,
        capabilities: Vec<String>,
        soul: String,
        created_by: String,
    ) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_id(),
            name,
            role,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()),
            soul,
            status: AgentPoStatus::Normal,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 获取 capabilities 为 Vec
    pub fn get_capabilities(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }

    /// 软删除
    pub fn soft_delete(&mut self, deleted_by: String) {
        self.status = AgentPoStatus::Deleted;
        self.modified_by = deleted_by;
        self.updated_at = current_timestamp();
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
