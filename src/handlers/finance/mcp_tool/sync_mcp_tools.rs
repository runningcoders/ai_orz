//! Handler: POST /api/v1/mcp-servers/{server_id}/tools/sync - Sync MCP tools.

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SyncMcpToolsRequest, SyncMcpToolsResponse};

/// Sync remote MCP tools from one server into local Tool records.
#[register_handler_tool(
    id = "sync_mcp_tools",
    name = "sync_mcp_tools",
    description = "Sync remote MCP tools from one server into local Tool records",
    params = "common::api::SyncMcpToolsRequest"
)]
#[generate_http_handler]
pub async fn sync_mcp_tools(
    ctx: RequestContext,
    params: SyncMcpToolsRequest,
) -> Result<SyncMcpToolsResponse> {
    let synced = domain()
        .mcp_tool_manage()
        .sync_mcp_tools(ctx, &params.server_id)
        .await?;

    Ok(SyncMcpToolsResponse { synced })
}
