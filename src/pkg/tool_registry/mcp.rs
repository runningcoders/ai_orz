//! MCP (Model Context Protocol) tool provider

use anyhow::{anyhow, Result};
use crate::models::tool::ToolPo;
use rig::tool::ToolDyn;
use serde::{Deserialize, Serialize};

/// MCP tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP server URL
    pub server_url: String,
    /// Optional API key
    pub api_key: Option<String>,
}

/// Build an MCP tool from ToolPo
pub fn build(_po: &ToolPo) -> Result<Box<dyn ToolDyn>> {
    // TODO: Implement MCP tool wrapper
    Err(anyhow!("MCP tool not implemented yet"))
}
