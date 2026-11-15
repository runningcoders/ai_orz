//! Task related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Task status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TaskStatus {
    /// Cancelled (can be considered deleted)
    Cancelled = 0,
    /// Pending, not started yet (waiting for DAG prerequisites / owner pickup)
    #[default]
    Pending = 1,
    /// In progress
    InProgress = 2,
    /// Completed
    Completed = 3,
    /// Archived (archived to history after summary)
    Archived = 4,
    /// 执行人已提交、等待项目 Owner 验收（2026-11-15 补齐设计稿既有第 6 态）
    ///
    /// ⚠️ 判别值**追加为 5，禁止插入中间**：DB 存量值、serde 变体名与前端
    /// `3 => 已完成` 一类硬编码都依赖 `0..=4` 稳定。此态不是终态：只有
    /// `Completed` 才会触发交付消息与 DAG 后继补发。
    PendingReview = 5,
}

impl TaskStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Cancelled,
            1 => Self::Pending,
            2 => Self::InProgress,
            3 => Self::Completed,
            4 => Self::Archived,
            5 => Self::PendingReview,
            _ => Self::default(),
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<i32> for TaskStatus {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for TaskStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Assignee type (who the task is assigned to)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
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
        Self::from_i32(v)
    }
}

impl From<i64> for AssigneeType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
