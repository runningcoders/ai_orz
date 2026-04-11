//! Agent related enums

use serde::{Serialize, Deserialize};
use sqlx::Type;

/// AgentPo status (for soft delete)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[derive(Type)]
#[sqlx(type_name = "INTEGER")]
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

impl From<i32> for AgentStatus {
    fn from(v: i32) -> Self {
        v.into()
    }
}

impl From<i64> for AgentStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// ModelProvider status (for soft delete)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[derive(Type)]
#[sqlx(type_name = "INTEGER")]
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

impl From<i32> for ModelProviderStatus {
    fn from(v: i32) -> Self {
        v.into()
    }
}

impl From<i64> for ModelProviderStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
