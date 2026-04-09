//! 用户角色枚举

/// 用户角色
///
/// - super_admin: 超级管理员，拥有组织所有权限
/// - admin: 管理员，可以管理用户和组织
/// - member: 普通成员，只能查看和使用资源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRole {
    /// 超级管理员
    SuperAdmin,
    /// 管理员
    Admin,
    /// 普通成员
    Member,
}

impl UserRole {
    /// 转换为字符串
    pub fn to_str(self) -> &'static str {
        match self {
            UserRole::SuperAdmin => "super_admin",
            UserRole::Admin => "admin",
            UserRole::Member => "member",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "super_admin" => Some(Self::SuperAdmin),
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            _ => None,
        }
    }
}

impl Default for UserRole {
    /// 默认角色是普通成员
    fn default() -> Self {
        Self::Member
    }
}

impl From<UserRole> for String {
    fn from(role: UserRole) -> Self {
        role.to_str().to_string()
    }
}
