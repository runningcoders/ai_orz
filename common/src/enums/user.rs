//! User related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// User role
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
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
            // 修复 E2E-4：非法值之前落 default()=SuperAdmin，是提权漏洞；
            // 改为最小权限 Member（未知角色宁低不高）
            _ => UserRole::Member,
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

    /// 获取上级角色（权限更高一级）
    ///
    /// 并查集角色继承体系：
    /// Member → Admin → SuperAdmin（根）
    ///
    /// 上级角色拥有下级角色的所有权限
    pub fn parent(&self) -> Option<UserRole> {
        match self {
            UserRole::SuperAdmin => None, // 根节点，没有上级
            UserRole::Admin => Some(UserRole::SuperAdmin),
            UserRole::Member => Some(UserRole::Admin),
        }
    }

    /// 查找权限根（并查集 find 操作，带路径压缩语义）
    ///
    /// 最终都会回到 SuperAdmin
    pub fn find_root(&self) -> UserRole {
        match self.parent() {
            Some(parent) => parent.find_root(),
            None => *self,
        }
    }

    /// 判断当前用户角色是否满足要求的最低角色权限
    ///
    /// 核心逻辑：从 min_role 向上遍历祖先链，如果路径上包含 user_role，则满足。
    /// 因为上级角色 = 下级角色权限 + 额外权限，所以上级总是满足下级的要求。
    ///
    /// # 示例
    /// user=Admin, min_role=Member → Member→Admin ✅ 满足
    /// user=SuperAdmin, min_role=Member → Member→Admin→SuperAdmin ✅ 满足
    /// user=Member, min_role=Admin → Admin→SuperAdmin ❌ 不满足
    /// user=Member, min_role=SuperAdmin → SuperAdmin ❌ 不满足
    pub fn has_permission(user_role: UserRole, min_role: UserRole) -> bool {
        let mut current = min_role;
        loop {
            if current == user_role {
                return true;
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => return false,
            }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_role_has_permission() {
        // SuperAdmin 满足所有要求
        assert!(UserRole::has_permission(
            UserRole::SuperAdmin,
            UserRole::SuperAdmin
        ));
        assert!(UserRole::has_permission(
            UserRole::SuperAdmin,
            UserRole::Admin
        ));
        assert!(UserRole::has_permission(
            UserRole::SuperAdmin,
            UserRole::Member
        ));

        // Admin 满足 Admin 和 Member 要求，不满足 SuperAdmin
        assert!(!UserRole::has_permission(
            UserRole::Admin,
            UserRole::SuperAdmin
        ));
        assert!(UserRole::has_permission(UserRole::Admin, UserRole::Admin));
        assert!(UserRole::has_permission(UserRole::Admin, UserRole::Member));

        // Member 只满足自身
        assert!(!UserRole::has_permission(
            UserRole::Member,
            UserRole::SuperAdmin
        ));
        assert!(!UserRole::has_permission(UserRole::Member, UserRole::Admin));
        assert!(UserRole::has_permission(UserRole::Member, UserRole::Member));
    }

    #[test]
    fn test_user_role_parent() {
        assert_eq!(UserRole::SuperAdmin.parent(), None);
        assert_eq!(UserRole::Admin.parent(), Some(UserRole::SuperAdmin));
        assert_eq!(UserRole::Member.parent(), Some(UserRole::Admin));
    }

    #[test]
    fn test_user_role_find_root() {
        assert_eq!(UserRole::SuperAdmin.find_root(), UserRole::SuperAdmin);
        assert_eq!(UserRole::Admin.find_root(), UserRole::SuperAdmin);
        assert_eq!(UserRole::Member.find_root(), UserRole::SuperAdmin);
    }

    #[test]
    fn test_user_role_from_i32_invalid_falls_to_member() {
        // 非法值落最小权限，绝不提权
        assert_eq!(UserRole::from_i32(3), UserRole::Member);
        assert_eq!(UserRole::from_i32(-1), UserRole::Member);
        assert_eq!(UserRole::from_i32(0), UserRole::SuperAdmin);
    }
}
