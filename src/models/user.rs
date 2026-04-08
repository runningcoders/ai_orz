//! User 持久化对象
//!
//! 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_USERS`]

use crate::pkg::constants::UserRole;
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
    pub role: String,
    /// 状态：0 = 禁用，1 = 启用
    pub status: i32,
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
        let now = current_timestamp();
        Self {
            id,
            organization_id,
            username,
            display_name,
            email,
            password_hash,
            role: role.to_str().to_string(),
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 获取用户角色
    pub fn user_role(&self) -> Option<UserRole> {
        UserRole::from_str(&self.role)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
