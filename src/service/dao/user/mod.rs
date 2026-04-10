//! User DAO 接口和实现

pub mod sqlite;

use crate::error::AppError;
use crate::models::user::UserPo;
use common::constants::RequestContext;

// ==================== 接口 ====================

/// User DAO 接口
pub trait UserDaoTrait: Send + Sync {
    /// 插入新用户
    fn insert(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError>;

    /// 根据 ID 查询用户
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>, AppError>;

    /// 根据用户名查询用户（用于登录）
    fn find_by_username(&self, ctx: RequestContext, username: &str) -> Result<Option<UserPo>, AppError>;

    /// 查询组织下所有用户
    fn find_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<Vec<UserPo>, AppError>;

    /// 更新用户
    fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError>;

    /// 删除用户（软删除）
    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 检查用户名是否已存在
    fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool, AppError>;

    /// 统计组织下用户总数
    fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64, AppError>;
}

pub use sqlite::dao;
pub use sqlite::init;

#[cfg(test)]
mod sqlite_test;
