//! User DAO 模块

use common::error::{Error, Result};
use crate::models::user::UserPo;
use crate::pkg::RequestContext;

/// User 查询参数
#[derive(Debug, Clone, Default)]
pub struct UserQuery {
    pub organization_id: Option<String>,
    pub limit: Option<usize>,
}

// ==================== 接口 ====================

/// User DAO trait
#[async_trait::async_trait]
pub trait UserDao: Send + Sync {
    /// 插入新用户
    async fn insert(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 根据 ID 查询用户
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>>;

    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>>;

    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<Vec<UserPo>>;

    /// 查询组织下所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>>;

    /// 更新用户
    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()>;

    /// 删除用户（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 检查用户名是否已存在
    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool>;

    /// 统计组织下用户总数
    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
