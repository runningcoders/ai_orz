//! HTTP remote tool provider

use anyhow::{anyhow, Result};
use crate::models::tool::ToolPo;
use rig::tool::ToolDyn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// HTTP endpoint
    pub endpoint: String,
    /// HTTP method, default POST
    pub method: Option<String>,
    /// Optional headers for authentication
    pub headers: Option<HashMap<String, String>>,
}

/// Build an HTTP tool from ToolPo
pub fn build(_po: &ToolPo) -> Result<Box<dyn ToolDyn>> {
    // TODO: Implement HTTP tool wrapper
    Err(anyhow!("HTTP tool not implemented yet"))
}
