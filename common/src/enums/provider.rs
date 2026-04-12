//! Model provider related enums

use serde::{Serialize, Deserialize};
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
