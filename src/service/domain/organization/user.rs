//! 用户管理 trait 实现
//!
//! 定义用户相关业务接口实现

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::user::UserDao;
use async_trait::async_trait;

#[async_trait]
impl super::UserManage for super::OrganizationDomainImpl {
    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>, AppError> {
        self.user_dao.find_by_username(ctx, username).await
    }

    /// 根据组织 ID 查询所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>, AppError> {
        self.user_dao.find_by_organization_id(ctx, org_id).await
    }

    /// 创建新用户
    async fn create_user(
        &self,
        ctx: RequestContext,
        user: UserPo,
    ) -> Result<(), AppError> {
        self.user_dao.insert(ctx, &user).await
    }

    /// 更新用户信息
    async fn update_user(
        &self,
        ctx: RequestContext,
        user: &UserPo,
    ) -> Result<(), AppError> {
        self.user_dao.update(ctx, user).await
    }

    /// 删除用户（软删除）
    async fn delete_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<(), AppError> {
        self.user_dao.delete(ctx, user_id).await
    }

    /// 检查用户名是否已存在
    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool, AppError> {
        self.user_dao.exists_by_username(ctx, username).await
    }

    /// 统计组织下用户总数
    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64, AppError> {
        self.user_dao.count_by_organization_id(ctx, org_id).await
    }

    /// 验证用户名密码（用于登录）
    async fn verify_password(
        &self,
        _ctx: RequestContext,
        org_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<UserPo, AppError> {
        // 先查找用户
        let user = match self.user_dao.find_by_username(_ctx, username).await? {
            Some(u) => u,
            None => {
                return Err(AppError::BadRequest("用户名或密码错误".to_string()));
            }
        };

        // 检查用户所属组织是否匹配
        if user.organization_id.as_str() != org_id {
            return Err(AppError::BadRequest("用户名或密码错误".to_string()));
        }

        // 验证密码哈希
        if user.password_hash.as_str() != password_hash {
            return Err(AppError::BadRequest("用户名或密码错误".to_string()));
        }

        // 用户状态检查：Active 表示启用
        if user.status != common::enums::UserStatus::Active {
            return Err(AppError::BadRequest("用户已被禁用".to_string()));
        }

        Ok(user)
    }

    /// 根据用户 ID 获取用户信息
    async fn get_user_by_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<UserPo>, AppError> {
        self.user_dao.find_by_id(ctx, user_id).await
    }
}
