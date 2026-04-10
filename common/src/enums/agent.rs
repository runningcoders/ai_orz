//! Agent related enums

use serde::{Serialize, Deserialize};
#[cfg(feature = "rusqlite")]
use rusqlite::{types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef}, Result as RusqliteResult};

/// AgentPo status (for soft delete)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentStatus {
    /// Deleted (soft deleted)
    Deleted = 0,
    /// Normal (available)
    #[default]
    Normal = 1,
}

impl AgentStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for AgentStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for AgentStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

/// ModelProvider status (for soft delete)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelProviderStatus {
    /// Deleted (soft deleted)
    Deleted = 0,
    /// Normal (available)
    #[default]
    Normal = 1,
}

impl ModelProviderStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for ModelProviderStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for ModelProviderStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}
