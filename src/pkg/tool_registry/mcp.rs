//! MCP (Model Context Protocol) tool provider.
//!
//! MCP tools are database-registered tools. `ToolPo.config` stores only the
//! binding from a standard tool record to an MCP server/tool pair:
//! `{ "server_id": "...", "tool_name": "..." }`.
//! Server connection details, credentials, headers, env, and commands belong to
//! `McpServerPo.config` and must not be duplicated into each MCP tool config.

use crate::models::mcp_server::McpServerPo;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use common::enums::ToolProtocol;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// MCP tool binding configuration stored in `ToolPo.config`.
///
/// This intentionally contains no server credentials or transport config. The
/// MCP-specific DAL loads the referenced `McpServerPo` and passes those deps to
/// the MCP runtime factory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpToolConfig {
    /// ID of the MCP server/provider record.
    pub server_id: String,
    /// Name of the concrete tool exposed by that MCP server.
    pub tool_name: String,
}

/// Minimal MCP client runtime skeleton.
///
/// Stage 3 owns the lifecycle boundary only. Real rmcp session management is
/// implemented in the next stage, but invalidation is already wired so update /
/// delete flows can depend on this boundary.
#[derive(Debug, Default)]
pub struct McpClientRuntime {
    invalidated_servers: Mutex<HashSet<String>>,
}

impl McpClientRuntime {
    pub fn invalidate_server(&self, server_id: &str) {
        self.invalidated_servers
            .lock()
            .unwrap()
            .insert(server_id.to_string());
    }

    #[cfg(test)]
    pub fn is_invalidated(&self, server_id: &str) -> bool {
        self.invalidated_servers.lock().unwrap().contains(server_id)
    }
}

/// Runtime dependencies needed to build an executable MCP CoreTool.
#[derive(Debug, Clone)]
pub struct McpToolDeps {
    pub server: McpServerPo,
    pub client_runtime: Arc<McpClientRuntime>,
}

/// Executable MCP core tool stub created from `ToolPo + McpToolConfig + deps`.
///
/// Stage 3 validates configuration and carries runtime deps. Actual rmcp
/// session/call execution is added in Stage 4.
#[derive(Debug, Clone)]
pub struct McpCoreTool {
    po: ToolPo,
    config: McpToolConfig,
    server: Option<McpServerPo>,
    client_runtime: Option<Arc<McpClientRuntime>>,
}

impl McpCoreTool {
    /// Build an MCP core tool stub from a persistent ToolPo.
    pub fn from_po(po: ToolPo) -> Result<Self> {
        let config = parse_and_validate_config(&po)?;
        Ok(Self {
            po,
            config,
            server: None,
            client_runtime: None,
        })
    }

    /// Build an MCP core tool with server/runtime deps prepared by McpToolDal.
    pub fn from_po_with_deps(po: ToolPo, deps: McpToolDeps) -> Result<Self> {
        let config = parse_and_validate_config(&po)?;
        if config.server_id != deps.server.id {
            return Err(anyhow!(
                "mcp tool server_id {} does not match MCP server {}",
                config.server_id,
                deps.server.id
            ));
        }

        Ok(Self {
            po,
            config,
            server: Some(deps.server),
            client_runtime: Some(deps.client_runtime),
        })
    }

    pub fn config(&self) -> &McpToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for McpCoreTool {
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value, ToolError> {
        let runtime_enabled = self.server.is_some() && self.client_runtime.is_some();
        let suffix = if runtime_enabled {
            "rmcp execution is not implemented yet"
        } else {
            "MCP runtime is not enabled"
        };
        Err(ToolError::ToolCallError(
            format!(
                "MCP tool {} on server {} is not executable: {}",
                self.config.tool_name, self.config.server_id, suffix
            )
            .into(),
        ))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Create an executable MCP tool stub from ToolPo.
pub fn create_tool(po: ToolPo) -> Result<Box<dyn CoreTool>> {
    Ok(Box::new(McpCoreTool::from_po(po)?))
}

/// Create an MCP tool using explicit server/runtime dependencies.
pub fn create_mcp_tool(po: ToolPo, deps: McpToolDeps) -> Result<Box<dyn CoreTool + Send + Sync>> {
    Ok(Box::new(McpCoreTool::from_po_with_deps(po, deps)?))
}

/// Validate an MCP ToolPo without constructing the executable runtime.
pub fn validate_tool_po_config(po: &ToolPo) -> Result<()> {
    parse_and_validate_config(po).map(|_| ())
}

fn parse_and_validate_config(po: &ToolPo) -> Result<McpToolConfig> {
    if po.protocol != ToolProtocol::Mcp {
        return Err(anyhow!("mcp tool factory only accepts ToolProtocol::Mcp"));
    }

    let config: McpToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid mcp tool config for {}: {}", po.id, e))?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &McpToolConfig) -> Result<()> {
    if config.server_id.trim().is_empty() {
        return Err(anyhow!("mcp tool server_id is required"));
    }

    if config.tool_name.trim().is_empty() {
        return Err(anyhow!("mcp tool tool_name is required"));
    }

    Ok(())
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;
