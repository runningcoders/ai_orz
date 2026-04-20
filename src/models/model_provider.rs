//! 模型提供商实体

use common::enums::{ModelProviderStatus, ProviderType};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;

/// 模型提供商持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelProviderPo {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub status: ModelProviderStatus,
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 模型提供商业务对象
///
/// 只包含持久化配置，不包含 Cortex，Cortex 由 Brain 持有
#[derive(Clone)]
pub struct ModelProvider {
    pub po: ModelProviderPo,
}

impl fmt::Debug for ModelProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelProvider")
            .field("po", &self.po)
            .finish()
    }
}

impl ModelProvider {
    /// 创建新的 ModelProvider
    pub fn new(
        name: String,
        provider_type: ProviderType,
        model_name: String,
        api_key: String,
        base_url: Option<String>,
        description: Option<String>,
        creator: String,
    ) -> Self {
        Self {
            po: ModelProviderPo::new(
                name,
                provider_type,
                model_name,
                api_key,
                base_url,
                description,
                creator,
            ),
        }
    }

    /// 从 PO 创建业务对象
    pub fn from_po(po: ModelProviderPo) -> Self {
        Self { po }
    }

    /// 获取 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取名称
    pub fn name(&self) -> &str {
        self.po.name.as_str()
    }

    /// 获取模型名称
    pub fn model_name(&self) -> &str {
        self.po.model_name.as_str()
    }

    /// 获取 API Key
    pub fn api_key(&self) -> &str {
        self.po.api_key.as_str()
    }

    /// 更新时间戳
    pub fn touch(&mut self, modifier: &str) {
        self.po.modified_by = modifier.to_string();
        self.po.updated_at = current_timestamp();
    }
}

impl ModelProviderPo {
    pub fn new(
        name: String,
        provider_type: ProviderType,
        model_name: String,
        api_key: String,
        base_url: Option<String>,
        description: Option<String>,
        creator: String,
    ) -> Self {
        Self {
            id: generate_id(),
            name,
            provider_type,
            model_name,
            api_key,
            base_url: base_url.map(|s| if s.is_empty() { None } else { Some(s) }).unwrap_or_default(),
            description: description.map(|s| if s.is_empty() { None } else { Some(s) }).unwrap_or_default(),
            status: ModelProviderStatus::Normal,
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
