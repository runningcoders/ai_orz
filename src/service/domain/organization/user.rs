//! 用户管理 trait 实现
//!
//! 定义用户相关业务接口实现

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao;

impl super::UserManage for super::OrganizationDomainImpl {
    /// 根据用户名查询用户（用于登录）
    fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>, AppError> {
        dao::user::dao().find_by_username(ctx, username)
    }

    /// 根据组织 ID 查询所有用户
    fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>, AppError> {
        dao::user::dao().find_by_organization_id(ctx, org_id)
    }

    /// 创建新用户
    fn create_user(
        &self,
        ctx: RequestContext,
        user: UserPo,
    ) -> Result<(), AppError> {
        dao::user::dao().insert(ctx, &user)
    }

    /// 更新用户信息
    fn update_user(
        &self,
        ctx: RequestContext,
        user: &UserPo,
    ) -> Result<(), AppError> {
        dao::user::dao().update(ctx, user)
    }

    /// 删除用户（软删除）
    fn delete_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<(), AppError> {
        dao::user::dao().delete(ctx, user_id)
    }

    /// 检查用户名是否已存在
    fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool, AppError> {
        dao::user::dao().exists_by_username(ctx, username)
    }

    /// 统计组织下用户总数
    fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64, AppError> {
        dao::user::dao().count_by_organization_id(ctx, org_id)
    }
}
