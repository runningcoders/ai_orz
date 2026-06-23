//! MCP (Model Context Protocol) tool provider.
//!
//! MCP tools are database-registered tools. `ToolPo.config` stores only the
//! binding from a standard tool record to an MCP server/tool pair:
//! `{ "server_id": "...", "tool_name": "..." }`.
//! Server connection details, credentials, headers, env, and commands belong to
//! `McpServerPo.config` and must not be duplicated into each MCP tool config.

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use common::enums::ToolProtocol;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP tool binding configuration stored in `ToolPo.config`.
///
/// This intentionally contains no server credentials or transport config. The
/// MCP-specific DAL will load the referenced `McpServerPo` and pass those deps
/// to a richer MCP factory in later stages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpToolConfig {
    /// ID of the MCP server/provider record.
    pub server_id: String,
    /// Name of the concrete tool exposed by that MCP server.
    pub tool_name: String,
}

/// Executable MCP core tool stub created from `ToolPo + McpToolConfig`.
///
/// Stage 2 only validates configuration and wires registry construction. Actual
/// rmcp session/call execution is added by later `McpToolCallDaoImpl` stages.
#[derive(Debug, Clone)]
pub struct McpCoreTool {
    po: ToolPo,
    config: McpToolConfig,
}

impl McpCoreTool {
    /// Build an MCP core tool stub from a persistent ToolPo.
    pub fn from_po(po: ToolPo) -> Result<Self> {
        if po.protocol != ToolProtocol::Mcp {
            return Err(anyhow!("mcp tool factory only accepts ToolProtocol::Mcp"));
        }

        let config: McpToolConfig = serde_json::from_value(po.config.clone())
            .map_err(|e| anyhow!("invalid mcp tool config for {}: {}", po.id, e))?;

        validate_config(&config)?;

        Ok(Self { po, config })
    }

    pub fn config(&self) -> &McpToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for McpCoreTool {
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::ToolCallError(
            format!(
                "MCP tool {} on server {} is not executable until MCP runtime is enabled",
                self.config.tool_name, self.config.server_id
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

/// Validate an MCP ToolPo without constructing the executable runtime.
pub fn validate_tool_po_config(po: &ToolPo) -> Result<()> {
    let config: McpToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid mcp tool config for {}: {}", po.id, e))?;
    validate_config(&config)
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
