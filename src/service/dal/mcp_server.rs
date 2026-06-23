//! MCP Server DAL.
//!
//! Business-facing MCP Server management layer. DAO remains responsible only for
//! persistence of `McpServerPo`; this DAL exposes the `McpServer` business entity
//! to upper layers and performs minimal configuration validation.

use crate::error::AppError;
use crate::models::mcp_server::{
    McpServer, McpServerPo, McpServerQuery, McpServerStatus, McpTransport,
};
use crate::pkg::RequestContext;
use crate::service::dao::mcp_server::McpServerDao;
use crate::service::dao::tool_call::{self, McpToolCallDao};
use async_trait::async_trait;
use common::api::PagedResult;
use std::sync::{Arc, OnceLock};

static MCP_SERVER_DAL: OnceLock<Arc<dyn McpServerDal + Send + Sync>> = OnceLock::new();

/// Get global MCP Server DAL.
pub fn dal() -> Arc<dyn McpServerDal + Send + Sync> {
    MCP_SERVER_DAL.get().cloned().unwrap()
}

/// Initialize global MCP Server DAL using global DAO singletons.
pub fn init() {
    use crate::service::dao::mcp_server;
    let _ = MCP_SERVER_DAL.set(new(mcp_server::dao(), tool_call::mcp_dao()));
}

/// Create MCP Server DAL with explicit dependencies.
pub fn new(
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
) -> Arc<dyn McpServerDal + Send + Sync> {
    Arc::new(McpServerDalImpl {
        mcp_server_dao,
        mcp_tool_call_dao,
    })
}

#[async_trait]
pub trait McpServerDal: Send + Sync {
    async fn create(&self, ctx: RequestContext, server: &McpServer) -> Result<(), AppError>;

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<McpServer>, AppError>;

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServer>, AppError>;

    async fn update(&self, ctx: RequestContext, server: &McpServer) -> Result<(), AppError>;

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<(), AppError>;

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}

pub struct McpServerDalImpl {
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
}

#[async_trait]
impl McpServerDal for McpServerDalImpl {
    async fn create(&self, ctx: RequestContext, server: &McpServer) -> Result<(), AppError> {
        validate_mcp_server_po(&server.po)?;
        let mut po = server.po.clone();
        let now = common::constants::utils::current_timestamp();
        let uid = Some(ctx.uid());
        po.created_at = now;
        po.updated_at = now;
        po.created_by = uid.clone();
        po.updated_by = uid;
        self.mcp_server_dao.insert(ctx, &po).await
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<McpServer>, AppError> {
        Ok(self
            .mcp_server_dao
            .find_by_id(ctx, id)
            .await?
            .map(McpServer::from_po))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServer>, AppError> {
        Ok(self
            .mcp_server_dao
            .query(ctx, query)
            .await?
            .map(McpServer::from_po))
    }

    async fn update(&self, ctx: RequestContext, server: &McpServer) -> Result<(), AppError> {
        validate_mcp_server_po(&server.po)?;
        let mut po = server.po.clone();
        po.touch(Some(ctx.uid()));
        self.mcp_server_dao.update(ctx, &po).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(&server.po.id);
        Ok(())
    }

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<(), AppError> {
        self.mcp_server_dao.set_status(ctx, id, status).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(id);
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.mcp_server_dao.delete(ctx, id).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(id);
        Ok(())
    }
}

fn validate_mcp_server_po(po: &McpServerPo) -> Result<(), AppError> {
    let config = po.config();
    match po.transport {
        McpTransport::Stdio => {
            if config
                .command
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest(format!(
                    "MCP stdio server {} requires command",
                    po.id
                )));
            }
        }
        McpTransport::StreamableHttp => {
            return Err(AppError::BadRequest(
                "MCP streamable_http server management is not available until HTTP security policy is implemented"
                    .to_string(),
            ));
        }
    }

    Ok(())
}
