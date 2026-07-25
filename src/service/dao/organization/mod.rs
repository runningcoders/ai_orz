//! Organization DAO 模块

use common::error::Result;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;

/// Organization 查询参数
#[derive(Debug, Clone, Default)]
pub struct OrganizationQuery {
    pub limit: Option<usize>,
}

/// Organization DAO 接口
#[async_trait::async_trait]
pub trait OrganizationDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn count_all(&self, ctx: RequestContext) -> Result<u64>;

    /// 统计符合查询条件的组织数量（复用 query 的 filter 逻辑，只跑 COUNT 不跑 LIST）
    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
