use serde::{Deserialize, Serialize};

/// OrganizationPo 持久化对象
///
/// # SQLite 表结构
/// ```sql
/// CREATE TABLE IF NOT EXISTS organizations (
///     id TEXT PRIMARY KEY,
///     name TEXT NOT NULL,
///     description TEXT NOT NULL DEFAULT '',
///     status INTEGER NOT NULL DEFAULT 1,        -- 0=已删除, 1=正常
///     created_by TEXT NOT NULL DEFAULT '',
///     modified_by TEXT NOT NULL DEFAULT '',
///     created_at INTEGER NOT NULL,
///     updated_at INTEGER NOT NULL
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationPo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: i32,          // 0=已删除, 1=正常
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl OrganizationPo {
    pub fn new(name: String, description: String, created_by: String) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_id(),
            name,
            description,
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
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
