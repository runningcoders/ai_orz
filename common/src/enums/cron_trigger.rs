//! Cron Trigger related enumerations shared by backend and frontend.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Cron trigger type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TriggerType {
    /// One-time trigger.
    Once = 0,
    /// Cron expression trigger.
    Cron = 1,
    /// Fixed interval trigger.
    Interval = 2,
}

impl Default for TriggerType {
    fn default() -> Self {
        Self::Cron
    }
}

impl From<i32> for TriggerType {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Once,
            1 => Self::Cron,
            2 => Self::Interval,
            _ => Self::Cron,
        }
    }
}

impl From<i64> for TriggerType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl TriggerType {
    /// Convert the trigger type to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}
