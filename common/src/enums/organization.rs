//! Organization related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Organization status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
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
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum OrganizationScope {
    /// Local (当前设备运行的组织)
    #[default]
    Local = 0,
    /// Remote (目录同步所得影子记录，未直接建联，仅知其存在)
    Remote = 1,
    /// Linked (与本组织直接建联，可通信；organization_links 表存在对应记录)
    Linked = 2,
}

impl From<i32> for OrganizationScope {
    fn from(v: i32) -> Self {
        match v {
            0 => OrganizationScope::Local,
            1 => OrganizationScope::Remote,
            2 => OrganizationScope::Linked,
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

/// Organization link status（组织连接状态）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum OrganizationLinkStatus {
    /// Active (连接有效，可通信)
    #[default]
    Active = 1,
    /// Revoked (已断联；对端影子记录降级为 Remote，保留历史审计线索)
    Revoked = 0,
}

impl From<i32> for OrganizationLinkStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => OrganizationLinkStatus::Revoked,
            1 => OrganizationLinkStatus::Active,
            _ => OrganizationLinkStatus::default(),
        }
    }
}

impl OrganizationLinkStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<OrganizationLinkStatus> for i32 {
    fn from(s: OrganizationLinkStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for OrganizationLinkStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三态值必须稳定：数据库里存的是整数值，改值等于改协议
    #[test]
    fn test_scope_discriminants_are_stable() {
        assert_eq!(OrganizationScope::Local as i32, 0);
        assert_eq!(OrganizationScope::Remote as i32, 1);
        assert_eq!(OrganizationScope::Linked as i32, 2);
    }

    #[test]
    fn test_scope_round_trip() {
        for scope in [
            OrganizationScope::Local,
            OrganizationScope::Remote,
            OrganizationScope::Linked,
        ] {
            assert_eq!(OrganizationScope::from(scope.to_i32()), scope);
            assert_eq!(OrganizationScope::from_i32(scope.to_i32()), scope);
        }
    }

    /// 未知整数值回退默认（Local），存量数据兼容
    #[test]
    fn test_scope_unknown_value_falls_back_to_default() {
        assert_eq!(OrganizationScope::from(-1), OrganizationScope::Local);
        assert_eq!(OrganizationScope::from(99), OrganizationScope::Local);
        assert_eq!(OrganizationScope::from(0i64) as i32, 0);
    }
}
