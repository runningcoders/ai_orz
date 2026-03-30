use serde::{Deserialize, Serialize};
use crate::pkg::constants::AgentPoStatus;

/// AgentPo 持久化对象
///
/// # SQLite 表结构
/// ```sql
/// CREATE TABLE IF NOT EXISTS agents (
///     id TEXT PRIMARY KEY,
///     name TEXT NOT NULL,
///     role TEXT NOT NULL DEFAULT '',
///     capabilities TEXT NOT NULL DEFAULT '[]',  -- JSON string
///     soul TEXT NOT NULL DEFAULT '',            -- 长文本
///     status INTEGER NOT NULL DEFAULT 1,        -- 0=已删除, 1=正常
///     created_by TEXT NOT NULL DEFAULT '',
///     modified_by TEXT NOT NULL DEFAULT '',
///     created_at INTEGER NOT NULL,              -- 数据库自动设置 CURRENT_TIMESTAMP
///     updated_at INTEGER NOT NULL              -- 数据库自动设置 CURRENT_TIMESTAMP
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: String,        // JSON string
    pub soul: String,                // 长文本
    pub status: AgentPoStatus,       // 软删除状态
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,            // 数据库自动设置
    pub updated_at: i64,            // 数据库自动设置
}

impl AgentPo {
    pub fn new(
        name: String,
        role: String,
        capabilities: Vec<String>,
        soul: String,
        created_by: String,
    ) -> Self {
        Self {
            id: generate_id(),
            name,
            role,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()),
            soul,
            status: AgentPoStatus::Normal,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: 0,  // 数据库自动设置
            updated_at: 0,  // 数据库自动设置
        }
    }

    pub fn get_capabilities(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random = rand_u32();
    format!("{:016x}{:08x}", timestamp, random)
}

fn rand_u32() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let time2 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;
    time2.wrapping_add(hasher.finish() as u32)
}
