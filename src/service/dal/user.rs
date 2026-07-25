//! User DAL 模块
//!
//! 职责：User 领域的数据访问层，封装 UserDao 提供统一的查询接口

use common::error::Result;
use common::api::PagedResult;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::user;
use crate::service::dao::user::{UserDao, UserQuery};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static USER_DAL: OnceLock<Arc<dyn UserDal + Send + Sync>> = OnceLock::new();

/// 获取 User DAL 单例
pub fn dal() -> Arc<dyn UserDal + Send + Sync> {
    USER_DAL.get().cloned().unwrap()
}

/// 初始化 User DAL
pub fn init() {
    let _ = USER_DAL.set(new(user::dao()));
}

/// 创建 User DAL（返回 trait 对象）
pub fn new(user_dao: Arc<dyn UserDao + Send + Sync>) -> Arc<dyn UserDal + Send + Sync> {
    Arc::new(UserDalImpl { user_dao })
}

// ==================== DAL 接口 ====================

/// User DAL 接口
#[async_trait::async_trait]
pub trait UserDal: Send + Sync {
    /// 创建用户
    async fn create(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 根据 ID 获取用户
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>>;

    /// 根据用户名获取用户
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>>;

    /// 通用综合查询
    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<PagedResult<UserPo>>;

    /// 获取组织下的所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>>;

    /// 更新用户信息
    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 删除用户（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 检查用户名是否存在
    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool>;

    /// 统计组织下的用户数量
    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64>;

    /// 统计符合查询条件的用户数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: UserQuery) -> Result<u64>;
}

// ==================== DAL 实现 ====================

/// User DAL 实现
struct UserDalImpl {
    user_dao: Arc<dyn UserDao + Send + Sync>,
}

#[async_trait::async_trait]
impl UserDal for UserDalImpl {
    async fn create(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dao.insert(ctx, user).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>> {
        self.user_dao.find_by_id(ctx, id).await
    }

    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>> {
        self.user_dao.find_by_username(ctx, username).await
    }

    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<PagedResult<UserPo>> {
        self.user_dao.query(ctx, query).await
    }

    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>> {
        let page = self
            .query(
                ctx,
                UserQuery {
                    organization_id: Some(org_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        self.user_dao.update(ctx, user).await
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.user_dao.delete(ctx, id).await
    }

    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool> {
        self.user_dao.exists_by_username(ctx, username).await
    }

    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            UserQuery {
                organization_id: Some(org_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: UserQuery) -> Result<u64> {
        self.user_dao.count(ctx, query).await
    }
}
