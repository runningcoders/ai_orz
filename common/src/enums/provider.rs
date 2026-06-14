//! Model provider related enums

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Model provider type
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ProviderType {
    /// OpenAI compatible
    #[default]
    OpenAI = 0,
    /// DeepSeek
    DeepSeek = 1,
    /// 通义千问
    Qwen = 2,
    /// 豆包
    Doubao = 3,
    /// Ollama local
    Ollama = 4,
    /// Custom OpenAI compatible
    Custom = 5,
    /// FastEmbed local embedding (纯本地向量化，无外部依赖)
    FastEmbed = 6,
}

/// Model capability type - 区分模型是用于 Agent 思考还是 Embedding 向量化
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ModelCapability {
    /// Agent 类型 - 支持对话、思考、工具调用
    #[default]
    Agent = 0,
    /// Embedding 类型 - 支持向量化
    Embedding = 1,
}

impl From<i32> for ProviderType {
    fn from(v: i32) -> Self {
        match v {
            0 => ProviderType::OpenAI,
            1 => ProviderType::DeepSeek,
            2 => ProviderType::Qwen,
            3 => ProviderType::Doubao,
            4 => ProviderType::Ollama,
            5 => ProviderType::Custom,
            6 => ProviderType::FastEmbed,
            _ => ProviderType::default(),
        }
    }
}

impl ProviderType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<ProviderType> for i32 {
    fn from(t: ProviderType) -> i32 {
        t as i32
    }
}

impl From<i64> for ProviderType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl From<i32> for ModelCapability {
    fn from(v: i32) -> Self {
        match v {
            0 => ModelCapability::Agent,
            1 => ModelCapability::Embedding,
            _ => ModelCapability::default(),
        }
    }
}

impl ModelCapability {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }

    /// Check if it's Agent capability
    pub fn is_agent(&self) -> bool {
        matches!(self, ModelCapability::Agent)
    }

    /// Check if it's Embedding capability
    pub fn is_embedding(&self) -> bool {
        matches!(self, ModelCapability::Embedding)
    }
}

impl From<ModelCapability> for i32 {
    fn from(t: ModelCapability) -> i32 {
        t as i32
    }
}

impl From<i64> for ModelCapability {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
