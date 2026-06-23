//! MCP Server related enumerations shared by backend and frontend.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// MCP Server transport type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum McpTransport {
    /// stdio transport: command plus args, without shell interpolation.
    Stdio = 0,
    /// streamable HTTP transport.
    StreamableHttp = 1,
}

impl Default for McpTransport {
    fn default() -> Self {
        Self::Stdio
    }
}

/// MCP Server management status.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum McpServerStatus {
    /// Soft-deleted server.
    Deleted = 0,
    /// Enabled server.
    Enabled = 1,
    /// Disabled server.
    Disabled = 2,
}

impl Default for McpServerStatus {
    fn default() -> Self {
        Self::Enabled
    }
}
