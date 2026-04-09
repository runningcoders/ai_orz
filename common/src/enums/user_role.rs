//! User role enumeration - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// User role in organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    /// Super administrator - can manage everything in the system
    SuperAdmin = 1,
    /// Organization administrator - can manage organization and users
    Admin = 2,
    /// Regular member - can access resources within organization
    Member = 3,
}

impl UserRole {
    /// Convert from i32 stored in database
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::SuperAdmin),
            2 => Some(Self::Admin),
            3 => Some(Self::Member),
            _ => None,
        }
    }

    /// Get role display name in Chinese
    pub fn display_name(self) -> &'static str {
        match self {
            Self::SuperAdmin => "超级管理员",
            Self::Admin => "管理员",
            Self::Member => "成员",
        }
    }
}
