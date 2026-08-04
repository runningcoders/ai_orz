//! MCP-specific ToolCall DAO implementation.
//!
//! This module is a protocol enhancement over the generic `ToolCallDao`.
//! Generic tools are delegated to the base implementation; MCP tools require
//! server/runtime dependencies and must be assembled through `McpToolCallDao`.

use super::ToolCallDao;
use crate::models::mcp_server::McpServerPo;
use crate::models::tool::{CoreTool, Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::mcp::{self, McpClientRuntime, McpToolDeps, RemoteMcpTool};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use anyhow::Result;
use async_trait::async_trait;
use common::enums::ToolProtocol;
use rig::tool::DynamicTool;
use serde_json::Value;
use std::sync::Arc;

/// MCP-specific ToolCall DAO contract.
///
/// MCP CoreTool construction needs `McpServerPo` and runtime dependencies, so it
/// is intentionally not exposed through the generic `assemble_core_tool(po)`
/// entrypoint.
#[async_trait]
pub trait McpToolCallDao: ToolCallDao {
    fn assemble_mcp_core_tool(
        &self,
        po: &ToolPo,
        server: &McpServerPo,
    ) -> Result<Option<Box<dyn CoreTool + Send + Sync>>>;

    fn invalidate_mcp_server(&self, server_id: &str);

    async fn list_mcp_tools(&self, server: &McpServerPo) -> Result<Vec<RemoteMcpTool>>;

    #[cfg(test)]
    fn is_mcp_server_invalidated(&self, server_id: &str) -> bool;
}

/// Create an MCP-enhanced ToolCall DAO with the default MCP client runtime.
pub fn new(base: Arc<dyn ToolCallDao + Send + Sync>) -> Arc<dyn McpToolCallDao + Send + Sync> {
    Arc::new(McpToolCallDaoImpl::new(
        base,
        Arc::new(McpClientRuntime::default()),
    ))
}

/// Create an MCP-enhanced ToolCall DAO with an injected runtime.
pub fn new_with_runtime(
    base: Arc<dyn ToolCallDao + Send + Sync>,
    client_runtime: Arc<McpClientRuntime>,
) -> Arc<dyn McpToolCallDao + Send + Sync> {
    Arc::new(McpToolCallDaoImpl::new(base, client_runtime))
}

#[derive(Clone)]
pub struct McpToolCallDaoImpl {
    base: Arc<dyn ToolCallDao + Send + Sync>,
    client_runtime: Arc<McpClientRuntime>,
}

impl McpToolCallDaoImpl {
    pub fn new(
        base: Arc<dyn ToolCallDao + Send + Sync>,
        client_runtime: Arc<McpClientRuntime>,
    ) -> Self {
        Self {
            base,
            client_runtime,
        }
    }
}

#[async_trait]
impl ToolCallDao for McpToolCallDaoImpl {
    fn assemble_core_tool(&self, po: &ToolPo) -> Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        if po.protocol == ToolProtocol::Mcp {
            // MCP tools need server config and runtime deps. The generic
            // entrypoint deliberately returns None so callers route through
            // McpToolDal / assemble_mcp_core_tool instead.
            return Ok(None);
        }

        self.base.assemble_core_tool(po)
    }

    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<DynamicTool> {
        self.base.wrap_for_rig(tools, ctx)
    }

    async fn call_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        self.base.call_manual(ctx, tool, args).await
    }
}

#[async_trait]
impl McpToolCallDao for McpToolCallDaoImpl {
    fn assemble_mcp_core_tool(
        &self,
        po: &ToolPo,
        server: &McpServerPo,
    ) -> Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        let deps = McpToolDeps {
            server: server.clone(),
            client_runtime: self.client_runtime.clone(),
        };
        let tool = mcp::create_mcp_tool(po.clone(), deps)?;
        Ok(Some(tool))
    }

    fn invalidate_mcp_server(&self, server_id: &str) {
        self.client_runtime.invalidate_server(server_id);
    }

    async fn list_mcp_tools(&self, server: &McpServerPo) -> Result<Vec<RemoteMcpTool>> {
        self.client_runtime
            .list_tools(server)
            .await
            .map_err(Into::into)
    }

    #[cfg(test)]
    fn is_mcp_server_invalidated(&self, server_id: &str) -> bool {
        self.client_runtime.is_invalidated(server_id)
    }
}
