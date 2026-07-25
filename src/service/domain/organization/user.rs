//! 用户管理 trait 实现
//!
//! 定义用户相关业务接口实现

use common::error::{bail_err, Result};
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;

#[async_trait]
impl super::UserManage for super::OrganizationDomainImpl {
    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>> {
        self.user_dal.find_by_username(ctx, username).await
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<common::api::PagedResult<UserPo>> {
        self.user_dal.query(ctx, query).await
    }

    /// 根据组织 ID 查询所有用户
    ///
    /// 调用 DAL 层 find_by_organization_id 方法
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>> {
        self.user_dal.find_by_organization_id(ctx, org_id).await
    }

    /// 创建新用户
    async fn create_user(&self, ctx: RequestContext, user: UserPo) -> Result<()> {
        self.user_dal.create(ctx, &user).await
    }

    /// 更新用户信息
    async fn update_user(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dal.update(ctx, user).await
    }

    /// 删除用户（软删除）
    async fn delete_user(&self, ctx: RequestContext, user_id: &str) -> Result<()> {
        self.user_dal.delete(ctx, user_id).await
    }

    /// 检查用户名是否已存在
    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool> {
        self.user_dal.exists_by_username(ctx, username).await
    }

    /// 统计组织下用户总数
    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64> {
        // 语法糖：调用通用 count_users
        self.count_users(
            ctx,
            crate::service::dao::user::UserQuery {
                organization_id: Some(org_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    /// 统计符合查询条件的用户数量（透传 DAL count）
    async fn count_users(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<u64> {
        self.user_dal.count(ctx, query).await
    }

    /// 验证用户名密码（用于登录）
    async fn verify_password(
        &self,
        _ctx: RequestContext,
        org_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<UserPo> {
        // 先查找用户
        let user = match self.user_dal.find_by_username(_ctx, username).await? {
            Some(u) => u,
            None => {
                bail_err!(InvalidRequest, "用户名或密码错误");
            }
        };

        // 检查用户所属组织是否匹配
        if user.organization_id.as_str() != org_id {
            bail_err!(InvalidRequest, "用户名或密码错误");
        }

        // 验证密码哈希
        if user.password_hash.as_str() != password_hash {
            bail_err!(InvalidRequest, "用户名或密码错误");
        }

        // 用户状态检查：Active 表示启用
        if user.status != common::enums::UserStatus::Active {
            bail_err!(InvalidRequest, "用户已被禁用");
        }

        Ok(user)
    }

    /// 根据用户 ID 获取用户信息
    async fn get_user_by_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<UserPo>> {
        self.user_dal.find_by_id(ctx, user_id).await
    }
}
