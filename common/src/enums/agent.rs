//! Agent related enums

use serde::{Serialize, Deserialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Agent status (lifecycle management)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AgentStatus {
    /// Deleted (soft deleted)
    Deleted = 0,
    /// Interviewing (draft status, being evaluated)
    #[default]
    Interviewing = 1,
    /// Pending onboarding, ready to be activated
    PendingOnboard = 2,
    /// Onboarded, fully active and available
    Onboarded = 3,
    /// Offboarded, no longer active
    Offboarded = 4,
}

impl AgentStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            1 => Self::Interviewing,
            2 => Self::PendingOnboard,
            3 => Self::Onboarded,
            4 => Self::Offboarded,
            _ => Self::Interviewing,
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
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
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
