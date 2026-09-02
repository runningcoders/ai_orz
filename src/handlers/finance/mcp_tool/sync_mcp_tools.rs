//! Handler: POST /api/v1/mcp-servers/{server_id}/tools/sync - Sync MCP tools.

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SyncMcpToolsRequest, SyncMcpToolsResponse};
use common::error::Result;

/// Sync remote MCP tools from one server into local Tool records.
#[register_handler_tool(
    id = "sync_mcp_tools",
    name = "Sync MCP Server Tools",
    description = "Pull the tool list from a remote MCP Server and upsert it into local Tool records: new tools are created, existing ones refreshed, and tools no longer offered are marked Stale. Returns the number of tools synced; fails if the server_id does not exist or the server is unreachable.",
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
