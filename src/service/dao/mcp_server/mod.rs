
//! MCP Server DAO 模块
//! Stage 1 仅包含 MCP Server 配置的纯持久化能力，不管理 MCP client/session 生命周期。

use common::error::Result;
use crate::models::mcp_server::{McpServerPo, McpServerQuery, McpServerStatus};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::PagedResult;
use std::sync::Arc;

/// MCP Server DAO 接口。
#[async_trait]
pub trait McpServerDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, server: &McpServerPo) -> Result<()>;

    async fn update(&self, ctx: RequestContext, server: &McpServerPo) -> Result<()>;

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<McpServerPo>>;

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServerPo>>;

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<()>;

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

mod sqlite;
#[cfg(test)]
mod sqlite_test;

pub use self::sqlite::{dao, init, new as new_mcp_server_dao};

/// Get global MCP Server DAO。
pub fn get() -> Arc<dyn McpServerDao + Send + Sync> {
    dao()
}
