//! Task related enums

use serde::{Serialize, Deserialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Task status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TaskStatus {
    /// Cancelled (can be considered deleted)
    Cancelled = 0,
    /// Pending, not started yet
    #[default]
    Pending = 1,
    /// In progress
    InProgress = 2,
    /// Completed
    Completed = 3,
}

impl TaskStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Cancelled,
            1 => Self::Pending,
            2 => Self::InProgress,
            _ => Self::Completed,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<i32> for TaskStatus {
    fn from(v: i32) -> Self {
        v.into()
    }
}

impl From<i64> for TaskStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Assignee type (who the task is assigned to)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AssigneeType {
    /// Assigned to User
    User = 0,
    /// Assigned to Agent
    #[default]
    Agent = 1,
}

impl AssigneeType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::User,
            _ => Self::Agent,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<i32> for AssigneeType {
    fn from(v: i32) -> Self {
        v.into()
    }
}

impl From<i64> for AssigneeType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
