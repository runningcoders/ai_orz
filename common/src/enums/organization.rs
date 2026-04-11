//! Organization related enums

use sqlx::Type;

/// Organization status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[derive(Type)]
#[sqlx(rename_all = "lowercase", type_name = "INTEGER")]
pub enum OrganizationStatus {
    /// Active (正常使用)
    #[default]
    Active = 1,
    /// Disabled (禁用/软删除)
    Disabled = 0,
}

impl From<i32> for OrganizationStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => OrganizationStatus::Disabled,
            1 => OrganizationStatus::Active,
            _ => OrganizationStatus::Active,
        }
    }
}

impl OrganizationStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<OrganizationStatus> for i32 {
    fn from(s: OrganizationStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for OrganizationStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Organization scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[derive(Type)]
#[sqlx(rename_all = "lowercase", type_name = "INTEGER")]
pub enum OrganizationScope {
    /// Local (当前设备运行)
    #[default]
    Local = 0,
    /// Remote (其他网络节点)
    Remote = 1,
}

impl From<i32> for OrganizationScope {
    fn from(v: i32) -> Self {
        match v {
            0 => OrganizationScope::Local,
            1 => OrganizationScope::Remote,
            _ => OrganizationScope::default(),
        }
    }
}

impl OrganizationScope {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<OrganizationScope> for i32 {
    fn from(s: OrganizationScope) -> i32 {
        s as i32
    }
}

impl From<i64> for OrganizationScope {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
