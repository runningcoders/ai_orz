//! MCP Server related API request/response DTOs - shared between backend and frontend.

use crate::api::PaginationParams;
use crate::enums::{McpServerStatus, McpTransport};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// MCP Server connection config DTO.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub struct McpServerConfigDto {
    /// stdio transport command.
    pub command: Option<String>,
    /// stdio transport args.
    #[serde(default)]
    pub args: Vec<String>,
    /// stdio transport explicit env vars. Defaults to not inheriting process env.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// streamable HTTP URL.
    pub url: Option<String>,
    /// streamable HTTP headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Call timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Connection timeout in milliseconds.
    pub connect_timeout_ms: Option<u64>,
    /// Maximum response body size in bytes.
    pub response_max_bytes: Option<u64>,
}

/// Create MCP Server request.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateMcpServerRequest {
    /// MCP Server display name.
    pub name: String,
    /// MCP Server transport.
    pub transport: McpTransport,
    /// MCP Server connection config.
    pub config: McpServerConfigDto,
}

/// Get MCP Server request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetMcpServerRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub id: String,
}

/// Delete MCP Server request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteMcpServerRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub id: String,
}

/// MCP Server list query parameters.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListMcpServersRequest {
    /// Filter by exact MCP Server ID.
    #[param(source = "query")]
    pub id: Option<String>,
    /// Filter by MCP Server name.
    #[param(source = "query")]
    pub name: Option<String>,
    /// Filter by transport.
    #[param(source = "query")]
    pub transport: Option<McpTransport>,
    /// Filter by status.
    #[param(source = "query")]
    pub status: Option<McpServerStatus>,
    /// Unified pagination parameters.
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// Update MCP Server request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMcpServerRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub id: String,
    /// New display name.
    pub name: Option<String>,
    /// New transport.
    pub transport: Option<McpTransport>,
    /// New connection config. `[REDACTED]` placeholders preserve stored secrets.
    pub config: Option<McpServerConfigDto>,
}

/// Update MCP Server status request.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMcpServerStatusRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub id: String,
    /// Target status. Deleted is not allowed via this API, use DELETE instead.
    pub status: McpServerStatus,
}

/// MCP Server list response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListMcpServersResponse {
    /// List of MCP Servers.
    pub servers: Vec<McpServerListItem>,
    /// Total count matching query.
    pub total: usize,
}

/// MCP Server list item response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpServerListItem {
    /// MCP Server ID.
    pub id: String,
    /// MCP Server display name.
    pub name: String,
    /// MCP Server transport.
    pub transport: McpTransport,
    /// Redacted management-safe config.
    pub config: McpServerConfigDto,
    /// MCP Server status.
    pub status: McpServerStatus,
    /// Created by user ID.
    pub created_by: Option<String>,
    /// Updated by user ID.
    pub updated_by: Option<String>,
    /// Created timestamp.
    pub created_at: i64,
    /// Updated timestamp.
    pub updated_at: i64,
}

/// MCP Server detail response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpServerDetail {
    /// MCP Server ID.
    pub id: String,
    /// MCP Server display name.
    pub name: String,
    /// MCP Server transport.
    pub transport: McpTransport,
    /// Redacted management-safe config.
    pub config: McpServerConfigDto,
    /// MCP Server status.
    pub status: McpServerStatus,
    /// Created by user ID.
    pub created_by: Option<String>,
    /// Updated by user ID.
    pub updated_by: Option<String>,
    /// Created timestamp.
    pub created_at: i64,
    /// Updated timestamp.
    pub updated_at: i64,
}

/// Create MCP Server response.
pub type CreateMcpServerResponse = McpServerDetail;

/// Get MCP Server response.
pub type GetMcpServerResponse = McpServerDetail;

/// Update MCP Server response.
pub type UpdateMcpServerResponse = McpServerDetail;

/// Update MCP Server status response.
pub type UpdateMcpServerStatusResponse = McpServerDetail;

/// Delete MCP Server response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteMcpServerResponse {
    /// Whether deletion succeeded.
    pub success: bool,
}
