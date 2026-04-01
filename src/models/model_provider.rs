//! 模型提供商实体

use crate::pkg::constants::{AgentPoStatus, ModelProviderStatus};
use serde::{Deserialize, Serialize};

/// 模型提供商类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    /// OpenAI 官方
    OpenAi,
    /// DeepSeek
    DeepSeek,
    /// 阿里云通义千问
    Qwen,
    /// 字节跳动豆包
    Doubao,
    /// 本地 Ollama
    Ollama,
    /// 其他 OpenAI 兼容接口
    OpenAiCompatible,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::OpenAi
    }
}

/// 模型提供商持久化对象
///
/// # SQLite 表结构
///
/// ```sql
/// CREATE TABLE IF NOT EXISTS model_providers (
///     id TEXT PRIMARY KEY,
///     name TEXT NOT NULL,
///     provider_type TEXT NOT NULL,
///     model_name TEXT NOT NULL,
///     api_key TEXT NOT NULL,
///     base_url TEXT,
///     description TEXT,
///     status INTEGER NOT NULL DEFAULT 1,
///     created_by TEXT NOT NULL DEFAULT '',
///     modified_by TEXT NOT NULL DEFAULT '',
///     created_at INTEGER NOT NULL,
///     updated_at INTEGER NOT NULL
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderPo {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub description: String,
    pub status: ModelProviderStatus,
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ModelProviderPo {
    pub fn new(
        name: String,
        provider_type: ProviderType,
        model_name: String,
        api_key: String,
        base_url: Option<String>,
        description: String,
        creator: String,
    ) -> Self {
        Self {
            id: generate_id(),
            name,
            provider_type,
            model_name,
            api_key,
            base_url,
            description,
            status: AgentPoStatus::Normal,
            created_by: creator.clone(),
            modified_by: creator,
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
        }
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

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
