//! Handler: GET /api/v1/mcp-servers/{server_id}/tools - List synced MCP tools.

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListMcpToolsByServerRequest, ListMcpToolsByServerResponse, ToolListItem};
use common::error::Result;

use super::super::tool::response::to_list_item;

/// List local MCP Tool records bound to one MCP Server.
#[register_handler_tool(
    id = "list_mcp_tools_by_server",
    name = "list_mcp_tools_by_server",
    description = "List local MCP Tool records bound to one MCP Server",
    params = "common::api::ListMcpToolsByServerRequest"
)]
#[generate_http_handler]
pub async fn list_mcp_tools_by_server(
    ctx: RequestContext,
    params: ListMcpToolsByServerRequest,
) -> Result<ListMcpToolsByServerResponse> {
    let result = domain()
        .mcp_tool_manage()
        .list_mcp_tools_by_server(ctx, params)
        .await?;

    let tools: Vec<ToolListItem> = result.items.iter().map(to_list_item).collect();
    Ok(ListMcpToolsByServerResponse {
        tools,
        total: result.total,
    })
}
