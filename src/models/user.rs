//! User 持久化对象
//!
//! 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_USERS`]

use common::constants::utils;
use common::enums::{UserRole, UserStatus};
use serde::{Deserialize, Serialize};

/// UserPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPo {
    /// 用户 ID
    pub id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 用户名（唯一登录名）
    pub username: String,
    /// 显示名称
    pub display_name: String,
    /// 邮箱
    pub email: String,
    /// 密码哈希（bcrypt）
    pub password_hash: String,
    /// 用户角色
    pub role: UserRole,
    /// 用户状态枚举
    pub status: UserStatus,
    /// 创建人
    pub created_by: String,
    /// 修改人
    pub modified_by: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

impl UserPo {
    /// 创建新的 UserPo
    pub fn new(
        id: String,
        organization_id: String,
        username: String,
        display_name: String,
        email: String,
        password_hash: String,
        role: UserRole,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        Self {
            id,
            organization_id,
            username,
            display_name,
            email,
            password_hash,
            role,
            status: UserStatus::default(),
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 获取用户角色（直接返回，不再需要转换）
    pub fn user_role(&self) -> Option<UserRole> {
        Some(self.role)
    }
}
