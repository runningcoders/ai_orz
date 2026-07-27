//! MCP Tool DAL.
//!
//! Protocol-specific DAL for MCP tools. It keeps MCP server lookup and runtime
//! dependency preparation out of the generic `ToolDal`.

use crate::models::mcp_server::McpServerStatus;
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::tool_registry::mcp::{McpToolConfig, RemoteMcpTool};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::service::dao::mcp_server::McpServerDao;
use crate::service::dao::tool::{ToolDao, ToolQuery};
use crate::service::dao::tool_call::{self, McpToolCallDao};
use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use common::api::{ListMcpToolsByServerRequest, PagedResult};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use common::error::{Result, bail_err, err};
use serde_json::Value;
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
    async fn get_by_id(&self, ctx: RequestContext, tool_id: String) -> Result<Option<Tool>>;

    /// Sync remote MCP tools from one server into standard ToolPo records.
    async fn sync_from_server(&self, ctx: RequestContext, server_id: &str) -> Result<usize>;

    /// List standard Tool records synced from one MCP Server.
    async fn list_by_server(
        &self,
        ctx: RequestContext,
        params: ListMcpToolsByServerRequest,
    ) -> Result<PagedResult<Tool>>;

    /// Execute an MCP tool by standard ToolPo id.
    /// 成功时返回 (Value, ToolCallEntry)，entry.call_id 为真实 call_id。
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)>;

    /// Execute an already assembled MCP tool.
    ///
    /// Use this when the caller already has the complete `Tool` entity so the
    /// execution path does not re-query tool metadata.
    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)>;

    /// Execute an already assembled MCP tool manually and return its trace entry.
    async fn call_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)>;

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
    async fn get_by_id(&self, ctx: RequestContext, tool_id: String) -> Result<Option<Tool>> {
        let Some(po) = self
            .tool_dao
            .get_by_id(ctx.clone(), tool_id.clone())
            .await?
        else {
            return Ok(None);
        };

        ensure_mcp_tool(&po)?;
        Ok(Some(self.assemble_executable_tool(ctx, &po).await?))
    }

    async fn sync_from_server(&self, ctx: RequestContext, server_id: &str) -> Result<usize> {
        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), server_id)
            .await?
        else {
            bail_err!(ResourceNotFound, "MCP server not found: {}", server_id);
        };

        let remote_tools = self.mcp_tool_call_dao.list_mcp_tools(&server).await?;
        let remote_tool_ids: std::collections::HashSet<String> = remote_tools
            .iter()
            .map(|remote_tool| mcp_tool_record_id(&server.id, &remote_tool.name))
            .collect();
        let mut synced = 0;

        for remote_tool in &remote_tools {
            let mut po = build_synced_tool_po(&server, remote_tool, ctx.user_id.clone());
            if let Some(existing) = self.tool_dao.get_by_id(ctx.clone(), po.id.clone()).await? {
                ensure_sync_target_matches(&existing, &po)?;
                po.created_at = existing.created_at;
                po.created_by = existing.created_by;
                po.status = if existing.status == ToolStatus::Stale {
                    ToolStatus::Enabled
                } else {
                    existing.status
                };
                po.updated_by = ctx.user_id.clone();
                self.tool_dao.update_tool(ctx.clone(), &po).await?;
            } else {
                self.tool_dao.create_tool(ctx.clone(), &po).await?;
            }
            synced += 1;
        }

        let existing_enabled_tools = self
            .tool_dao
            .query(
                ctx.clone(),
                ToolQuery {
                    protocol: Some(ToolProtocol::Mcp),
                    status: Some(ToolStatus::Enabled),
                    mcp_server_id: Some(server.id.clone()),
                    ..Default::default()
                },
            )
            .await?;

        for mut existing in existing_enabled_tools.items {
            if remote_tool_ids.contains(&existing.id) {
                continue;
            }
            existing.status = ToolStatus::Stale;
            existing.updated_by = ctx.user_id.clone();
            existing.touch(ctx.user_id.clone());
            self.tool_dao.update_tool(ctx.clone(), &existing).await?;
        }

        Ok(synced)
    }

    async fn list_by_server(
        &self,
        ctx: RequestContext,
        params: ListMcpToolsByServerRequest,
    ) -> Result<PagedResult<Tool>> {
        let server_exists = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), &params.server_id)
            .await?
            .is_some();
        if !server_exists {
            bail_err!(
                ResourceNotFound,
                "MCP server not found: {}",
                params.server_id
            );
        }

        let base_query = ToolQuery {
            keyword: params.keyword.clone(),
            protocol: Some(ToolProtocol::Mcp),
            status: params.status,
            mcp_server_id: Some(params.server_id.clone()),
            ..Default::default()
        };

        let all = self.tool_dao.query(ctx.clone(), base_query.clone()).await?;
        let total = all.total;

        let page = self
            .tool_dao
            .query(
                ctx,
                ToolQuery {
                    pagination: params.pagination,
                    ..base_query
                },
            )
            .await?;

        Ok(PagedResult {
            items: page
                .items
                .into_iter()
                .map(Tool::from_po_for_management)
                .collect(),
            total,
        })
    }

    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        let tool = self
            .get_by_id(ctx.clone(), tool_id.clone())
            .await
            .map_err(|e| common::error::Error::tool_call_failed(e.to_string()).with_source(e))?
            .ok_or_else(|| {
                common::error::Error::tool_call_failed(format!("Tool not found: {tool_id}"))
            })?;

        self.call_tool(ctx, &tool, args).await
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        self.call_manual(ctx, tool, args).await
    }

    async fn call_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        let executable = self
            .assemble_executable_tool(ctx.clone(), &tool.po)
            .await
            .map_err(|e| common::error::Error::tool_call_failed(e.to_string()).with_source(e))?;
        self.mcp_tool_call_dao
            .call_manual(ctx, &executable, args)
            .await
            .map_err(Into::into)
    }

    fn invalidate_server(&self, server_id: &str) {
        self.mcp_tool_call_dao.invalidate_mcp_server(server_id);
    }
}

impl McpToolDalImpl {
    async fn assemble_executable_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<Tool> {
        ensure_mcp_tool(po)?;
        ensure_mcp_tool_enabled(po)?;
        let config = parse_mcp_tool_config(po)?;

        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx, &config.server_id)
            .await?
        else {
            bail_err!(
                ResourceNotFound,
                "MCP server not found for tool {}: {}",
                po.id,
                config.server_id
            );
        };
        ensure_mcp_server_enabled(&server)?;

        let Some(our_tool) = self.mcp_tool_call_dao.assemble_mcp_core_tool(po, &server)? else {
            bail_err!(InvalidRequest, "MCP tool is not executable: {}", po.id);
        };

        Ok(Tool {
            po: po.clone(),
            our_tool,
            search_match: None,
            stats: None,
        })
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

fn ensure_mcp_tool(po: &ToolPo) -> Result<()> {
    if po.protocol != ToolProtocol::Mcp {
        bail_err!(InvalidRequest, "Tool {} is not an MCP tool", po.id);
    }
    Ok(())
}

fn ensure_mcp_tool_enabled(po: &ToolPo) -> Result<()> {
    if po.status != ToolStatus::Enabled {
        bail_err!(InvalidRequest, "MCP tool disabled: {}", po.id);
    }
    Ok(())
}

fn ensure_mcp_server_enabled(server: &crate::models::mcp_server::McpServerPo) -> Result<()> {
    if server.status != McpServerStatus::Enabled {
        bail_err!(InvalidRequest, "MCP server disabled: {}", server.id);
    }
    Ok(())
}

fn ensure_sync_target_matches(existing: &ToolPo, synced: &ToolPo) -> Result<()> {
    if existing.protocol != ToolProtocol::Mcp {
        bail_err!(
            Conflict,
            "MCP sync target id {} already exists as non-MCP tool",
            existing.id
        );
    }

    let existing_config = parse_mcp_tool_config(existing)?;
    let synced_config = parse_mcp_tool_config(synced)?;
    if existing_config.server_id != synced_config.server_id
        || existing_config.tool_name != synced_config.tool_name
    {
        bail_err!(
            Conflict,
            "MCP sync target id {} already binds to {}/{}",
            existing.id,
            existing_config.server_id,
            existing_config.tool_name
        );
    }

    Ok(())
}

fn parse_mcp_tool_config(po: &ToolPo) -> Result<McpToolConfig> {
    let config: AnyhowResult<McpToolConfig> = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow::anyhow!("invalid mcp tool config for {}: {}", po.id, e));
    config.map_err(|e| {
        err!(
            InvalidRequest,
            "invalid mcp tool config for {}: {}",
            po.id,
            e
        )
        .with_source::<common::error::Error>(e.into())
    })
}
