//! MCP Tool related API request/response DTOs - shared between backend and frontend.

use crate::api::PaginationParams;
use crate::api::ToolListItem;
use crate::enums::ToolStatus;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Sync remote MCP tools from a server into local Tool records.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SyncMcpToolsRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub server_id: String,
}

/// Sync remote MCP tools response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyncMcpToolsResponse {
    /// Number of remote tools synced/upserted.
    pub synced: usize,
}

/// List local MCP Tool records bound to one MCP Server.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListMcpToolsByServerRequest {
    /// MCP Server ID.
    #[param(source = "path")]
    pub server_id: String,
    /// Optional keyword filter over tool name/description.
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// Optional tool status filter.
    #[param(source = "query")]
    pub status: Option<ToolStatus>,
    /// Unified pagination parameters.
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// List local MCP Tool records response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListMcpToolsByServerResponse {
    /// Current page of tools.
    pub tools: Vec<ToolListItem>,
    /// Total count matching query, ignoring pagination.
    pub total: usize,
}
