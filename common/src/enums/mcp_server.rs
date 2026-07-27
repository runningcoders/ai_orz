//! MCP Server related enumerations shared by backend and frontend.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// MCP Server transport type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub enum McpTransport {
    /// stdio transport: command plus args, without shell interpolation.
    #[default]
    Stdio = 0,
    /// streamable HTTP transport.
    StreamableHttp = 1,
}

impl fmt::Display for McpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpTransport::Stdio => write!(f, "stdio"),
            McpTransport::StreamableHttp => write!(f, "streamable_http"),
        }
    }
}

/// MCP Server management status.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub enum McpServerStatus {
    /// Soft-deleted server.
    Deleted = 0,
    /// Enabled server.
    #[default]
    Enabled = 1,
    /// Disabled server.
    Disabled = 2,
}

impl fmt::Display for McpServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpServerStatus::Deleted => write!(f, "deleted"),
            McpServerStatus::Enabled => write!(f, "enabled"),
            McpServerStatus::Disabled => write!(f, "disabled"),
        }
    }
}
