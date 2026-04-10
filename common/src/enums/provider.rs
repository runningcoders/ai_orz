//! Model provider related enums

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// Model provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

impl ToSql for ProviderType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::from(*self as i32))
    }
}

impl FromSql for ProviderType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i32::column_result(value).map(|v| v.into())
    }
}
