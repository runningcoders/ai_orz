//! User DAO 模块

use crate::error::AppError;
use crate::models::user::UserPo;
use sqlx::SqlitePool;
use common::enums::{UserRole, UserStatus};
use crate::pkg::RequestContext;
use chrono::Utc;

// ==================== 接口 ====================

/// User DAO trait
#[async_trait::async_trait]
pub trait UserDaoTrait: Send + Sync {
    /// 插入新用户
    async fn insert(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError>;

    /// 根据 ID 查询用户
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>, AppError>;

    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(&self, ctx: RequestContext, username: &str) -> Result<Option<UserPo>, AppError>;

    /// 查询组织下所有用户
    async fn find_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<Vec<UserPo>, AppError>;

    /// 更新用户
    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError>;

    /// 删除用户（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 检查用户名是否已存在
    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool, AppError>;

    /// 统计组织下用户总数
    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64, AppError>;
}



mod sqlite;
pub use self::sqlite::{dao,init};

#[cfg(test)]
mod sqlite_test;