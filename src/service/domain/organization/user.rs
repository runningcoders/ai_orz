//! User 用户管理领域
//!
//! 包含用户管理相关功能：
//! - 根据用户名查询用户（用于登录）
//! - 根据组织 ID 查询所有用户
//! - 创建用户
//! - 更新用户
//! - 删除用户
//! - 检查用户名是否已存在
//! - 统计组织下用户总数

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::user;
use std::sync::Arc;

/// User 领域接口
pub trait UserDomain: Send + Sync {
    /// 根据用户名查询用户（用于登录）
    fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>, AppError>;

    /// 根据组织 ID 查询所有用户
    fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>, AppError>;

    /// 创建新用户
    fn create_user(
        &self,
        ctx: RequestContext,
        user: UserPo,
    ) -> Result<(), AppError>;

    /// 更新用户信息
    fn update_user(
        &self,
        ctx: RequestContext,
        user: &UserPo,
    ) -> Result<(), AppError>;

    /// 删除用户（软删除）
    fn delete_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<(), AppError>;

    /// 检查用户名是否已存在
    fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool, AppError>;

    /// 统计组织下用户总数
    fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64, AppError>;
}

/// User 领域实现
pub struct UserDomainImpl {
    dao: Arc<dyn user::UserDaoTrait + Send + Sync>,
}

impl UserDomainImpl {
    /// 创建 domain 实例
    pub fn new(dao: Arc<dyn user::UserDaoTrait + Send + Sync>) -> Self {
        Self { dao }
    }
}

impl UserDomain for UserDomainImpl {
    fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>, AppError> {
        self.dao.find_by_username(ctx, username)
    }

    fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>, AppError> {
        self.dao.find_by_organization_id(ctx, org_id)
    }

    fn create_user(
        &self,
        ctx: RequestContext,
        user: UserPo,
    ) -> Result<(), AppError> {
        self.dao.insert(ctx, &user)
    }

    fn update_user(
        &self,
        ctx: RequestContext,
        user: &UserPo,
    ) -> Result<(), AppError> {
        self.dao.update(ctx, user)
    }

    fn delete_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<(), AppError> {
        self.dao.delete(ctx, user_id)
    }

    fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool, AppError> {
        self.dao.exists_by_username(ctx, username)
    }

    fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64, AppError> {
        self.dao.count_by_organization_id(ctx, org_id)
    }
}
