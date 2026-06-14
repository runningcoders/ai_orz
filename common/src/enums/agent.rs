//! Agent related enums

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Agent 状态（生命周期管理）
///
/// 状态流转：
/// Interviewing → PendingOnboard → Onboarded → PendingOffboard → Offboarded
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AgentStatus {
    /// 已删除
    Deleted = 0,
    /// 面试中（创建 Agent 时的默认状态）
    #[default]
    Interviewing = 1,
    /// 待入职（确认入职，正在初始化）
    PendingOnboard = 2,
    /// 已入职（正常可用状态）
    Onboarded = 3,
    /// 已离职
    Offboarded = 4,
    /// 待离职（交接中，不接受新任务）
    PendingOffboard = 5,
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
            5 => Self::PendingOffboard,
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
        Self::from_i32(v)
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
