//! User related enums

use serde::{Serialize, Deserialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// User role
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum UserRole {
    /// Super admin (超级管理员)
    #[default]
    SuperAdmin = 0,
    /// Admin (管理员)
    Admin = 1,
    /// Member (普通成员)
    Member = 2,
}

impl From<i32> for UserRole {
    fn from(v: i32) -> Self {
        match v {
            0 => UserRole::SuperAdmin,
            1 => UserRole::Admin,
            2 => UserRole::Member,
            _ => UserRole::default(),
        }
    }
}

impl UserRole {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            UserRole::SuperAdmin => "超级管理员",
            UserRole::Admin => "管理员",
            UserRole::Member => "普通成员",
        }
    }
}

impl From<UserRole> for i32 {
    fn from(r: UserRole) -> i32 {
        r as i32
    }
}

impl From<i64> for UserRole {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// User status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum UserStatus {
    /// Active (正常使用)
    #[default]
    Active = 1,
    /// Disabled (禁用/软删除)
    Disabled = 0,
}

impl From<i32> for UserStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => UserStatus::Disabled,
            1 => UserStatus::Active,
            _ => UserStatus::Active,
        }
    }
}

impl UserStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<UserStatus> for i32 {
    fn from(s: UserStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for UserStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
