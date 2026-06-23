//! MCP Tool DAL.
//!
//! Protocol-specific DAL for MCP tools. It keeps MCP server lookup and runtime
//! dependency preparation out of the generic `ToolDal`.

use crate::error::AppError;
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::tool_registry::mcp::{McpToolConfig, RemoteMcpTool};
use crate::service::dao::mcp_server::McpServerDao;
use crate::service::dao::tool::ToolDao;
use crate::service::dao::tool_call::{self, McpToolCallDao};
use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol};
use std::sync::{Arc, OnceLock};

static MCP_TOOL_DAL: OnceLock<Arc<dyn McpToolDal + Send + Sync>> = OnceLock::new();

/// Get global MCP Tool DAL.
pub fn dal() -> Arc<dyn McpToolDal + Send + Sync> {
    MCP_TOOL_DAL.get().cloned().unwrap()
}

/// Initialize global MCP Tool DAL using global DAO singletons.
pub fn init() {
    use crate::service::dao::{mcp_server, tool};
    let _ = MCP_TOOL_DAL.set(new(tool::dao(), mcp_server::dao(), tool_call::mcp_dao()));
}

/// Create MCP Tool DAL with explicit dependencies.
pub fn new(
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
) -> Arc<dyn McpToolDal + Send + Sync> {
    Arc::new(McpToolDalImpl {
        tool_dao,
        mcp_server_dao,
        mcp_tool_call_dao,
    })
}

#[async_trait]
pub trait McpToolDal: Send + Sync {
    /// Get a complete executable MCP Tool by standard ToolPo id.
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
    ) -> Result<Option<Tool>, AppError>;

    /// Sync remote MCP tools from one server into standard ToolPo records.
    async fn sync_from_server(
        &self,
        ctx: RequestContext,
        server_id: &str,
    ) -> Result<usize, AppError>;

    /// Invalidate cached MCP runtime/session for a server.
    fn invalidate_server(&self, server_id: &str);
}

pub struct McpToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
}

#[async_trait]
impl McpToolDal for McpToolDalImpl {
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
    ) -> Result<Option<Tool>, AppError> {
        let Some(po) = self
            .tool_dao
            .get_by_id(ctx.clone(), tool_id.clone())
            .await
            .map_err(AppError::from)?
        else {
            return Ok(None);
        };

        ensure_mcp_tool(&po)?;
        let config = parse_mcp_tool_config(&po)?;

        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), &config.server_id)
            .await?
        else {
            return Err(AppError::NotFound(format!(
                "MCP server not found for tool {}: {}",
                po.id, config.server_id
            )));
        };

        let Some(our_tool) = self
            .mcp_tool_call_dao
            .assemble_mcp_core_tool(&po, &server)
            .map_err(AppError::from)?
        else {
            return Ok(Some(Tool::from_po_for_management(po)));
        };

        Ok(Some(Tool {
            po,
            our_tool,
            search_match: None,
        }))
    }

    async fn sync_from_server(
        &self,
        ctx: RequestContext,
        server_id: &str,
    ) -> Result<usize, AppError> {
        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), server_id)
            .await?
        else {
            return Err(AppError::NotFound(format!(
                "MCP server not found: {}",
                server_id
            )));
        };

        let remote_tools = self
            .mcp_tool_call_dao
            .list_mcp_tools(&server)
            .await
            .map_err(AppError::from)?;
        let mut synced = 0;

        for remote_tool in remote_tools {
            let mut po = build_synced_tool_po(&server, &remote_tool, ctx.user_id.clone());
            if let Some(existing) = self
                .tool_dao
                .get_by_id(ctx.clone(), po.id.clone())
                .await
                .map_err(AppError::from)?
            {
                ensure_sync_target_matches(&existing, &po)?;
                po.created_at = existing.created_at;
                po.created_by = existing.created_by;
                po.status = existing.status;
                po.updated_by = ctx.user_id.clone();
                self.tool_dao
                    .update_tool(ctx.clone(), &po)
                    .await
                    .map_err(AppError::from)?;
            } else {
                self.tool_dao
                    .create_tool(ctx.clone(), &po)
                    .await
                    .map_err(AppError::from)?;
            }
            synced += 1;
        }

        Ok(synced)
    }

    fn invalidate_server(&self, server_id: &str) {
        self.mcp_tool_call_dao.invalidate_mcp_server(server_id);
    }
}

fn build_synced_tool_po(
    server: &crate::models::mcp_server::McpServerPo,
    remote_tool: &RemoteMcpTool,
    creator: Option<String>,
) -> ToolPo {
    let id = mcp_tool_record_id(&server.id, &remote_tool.name);
    let description = remote_tool
        .description
        .clone()
        .unwrap_or_else(|| format!("MCP tool {} from server {}", remote_tool.name, server.name));
    let mut po = ToolPo::new(
        id.clone(),
        id,
        description,
        ToolProtocol::Mcp,
        serde_json::json!({
            "server_id": server.id,
            "tool_name": remote_tool.name,
        }),
        Some(remote_tool.input_schema.clone()),
        vec![
            "mcp".to_string(),
            server.id.clone(),
            remote_tool.name.clone(),
        ],
        creator,
    );
    po.control_mode = ControlMode::Manual;
    po
}

fn mcp_tool_record_id(server_id: &str, tool_name: &str) -> String {
    format!("mcp.{}.{}", server_id, tool_name)
}

fn ensure_mcp_tool(po: &ToolPo) -> Result<(), AppError> {
    if po.protocol != ToolProtocol::Mcp {
        return Err(AppError::BadRequest(format!(
            "Tool {} is not an MCP tool",
            po.id
        )));
    }
    Ok(())
}

fn ensure_sync_target_matches(existing: &ToolPo, synced: &ToolPo) -> Result<(), AppError> {
    if existing.protocol != ToolProtocol::Mcp {
        return Err(AppError::Conflict(format!(
            "MCP sync target id {} already exists as non-MCP tool",
            existing.id
        )));
    }

    let existing_config = parse_mcp_tool_config(existing)?;
    let synced_config = parse_mcp_tool_config(synced)?;
    if existing_config.server_id != synced_config.server_id
        || existing_config.tool_name != synced_config.tool_name
    {
        return Err(AppError::Conflict(format!(
            "MCP sync target id {} already binds to {}/{}",
            existing.id, existing_config.server_id, existing_config.tool_name
        )));
    }

    Ok(())
}

fn parse_mcp_tool_config(po: &ToolPo) -> Result<McpToolConfig, AppError> {
    let config: AnyhowResult<McpToolConfig> = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow::anyhow!("invalid mcp tool config for {}: {}", po.id, e));
    config.map_err(AppError::from)
}
