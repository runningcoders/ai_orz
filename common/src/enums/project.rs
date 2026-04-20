//! Project status enum

use serde::{Serialize, Deserialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Project status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ProjectStatus {
    /// Deleted (soft deleted, filtered out by default)
    Deleted = 0,
    /// Active (active and available)
    #[default]
    Active = 1,
    /// PendingReview (created by Agent, waiting for user review/approval)
    PendingReview = 2,
    /// InProgress (work is ongoing)
    InProgress = 3,
    /// Completed (work is done)
    Completed = 4,
    /// Archived (archived to history)
    Archived = 5,
}

impl From<i32> for ProjectStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => ProjectStatus::Deleted,
            1 => ProjectStatus::Active,
            2 => ProjectStatus::PendingReview,
            3 => ProjectStatus::InProgress,
            4 => ProjectStatus::Completed,
            5 => ProjectStatus::Archived,
            _ => ProjectStatus::default(),
        }
    }
}

impl ProjectStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<ProjectStatus> for i32 {
    fn from(s: ProjectStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for ProjectStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
