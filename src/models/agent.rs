//! Agent 实体

use crate::models::brain::{Brain, Cortex, CortexTrait};
use crate::pkg::constants::AgentPoStatus;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Agent 业务对象（DAL 层）
///
/// 组合 AgentPo 和其他相关信息，作为业务层的核心对象
/// 后续可扩展：执行环境、权限、配置等字段
pub struct Agent {
    /// 底层持久化对象
    pub po: AgentPo,
    /// 装配好的 Brain（推理执行实体）
    ///
    /// 如果为 None，表示还没有装配，需要调用 AgentDal::wake_brain 装配
    pub brain: Option<Brain>,
    // 后续扩展字段：
    // pub execution_env: ExecutionEnv,
    // pub permissions: Vec<Permission>,
    // pub config: AgentConfig,
}

impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agent")
            .field("po", &self.po)
            .field("brain", &"[Brain]")
            .finish()
    }
}

impl Agent {
    /// 从 Po 创建 Agent
    pub fn from_po(po: AgentPo) -> Self {
        Self {
            po,
            brain: None,
        }
    }

    /// 转换为 Po
    pub fn into_po(self) -> AgentPo {
        self.po
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取 Agent 名称
    pub fn name(&self) -> &str {
        &self.po.name
    }

    /// 获取模型提供商 ID
    pub fn model_provider_id(&self) -> &str {
        &self.po.model_provider_id
    }

    /// 设置装配好的 Brain
    pub fn set_brain(&mut self, brain: Brain) {
        self.brain = Some(brain);
    }

    /// 获取 Brain 引用
    pub fn brain(&self) -> Option<&Brain> {
        self.brain.as_ref()
    }

    /// 获取 Brain 内部的 Cortex 引用
    pub fn cortex(&self) -> Option<&Cortex> {
        self.brain.as_ref().map(|b| b.cortex())
    }

    /// 获取 Cortex 内部的 CortexTrait 引用
    pub fn cortex_trait(&self) -> Option<&(dyn CortexTrait + Send + Sync)> {
        self.brain.as_ref().map(|b| b.cortex_trait())
    }
}

/// AgentPo 持久化对象
///
/// 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_AGENTS`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: String, // JSON string
    pub soul: String,          // 长文本
    pub model_provider_id: String, // 关联模型提供商 ID
    pub status: AgentPoStatus, // 软删除状态
    pub created_by: String,    // 创建者
    pub modified_by: String,   // 修改者
    pub created_at: i64,
    pub updated_at: i64,
}

impl AgentPo {
    pub fn new(
        name: String,
        role: String,
        capabilities: Vec<String>,
        soul: String,
        model_provider_id: String,
        creator: String,
    ) -> Self {
        Self {
            id: generate_id(),
            name,
            role,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()),
            soul,
            model_provider_id,
            status: AgentPoStatus::Normal,
            created_by: creator.clone(),
            modified_by: creator,
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
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

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
