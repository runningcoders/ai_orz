//! Handler: GET /api/v1/mcp-servers/{server_id}/tools - List synced MCP tools.

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListMcpToolsByServerRequest, ListMcpToolsByServerResponse, ToolListItem};
use common::error::Result;

use super::super::tool::response::{probe_runtime_ready, to_list_item};

/// List local MCP Tool records bound to one MCP Server.
#[register_handler_tool(
    id = "list_mcp_tools_by_server",
    name = "List Server's MCP Tools",
    description = "List the local Tool records synced from one MCP Server, with optional keyword and status filters and pagination; returns {tools, total}. If the server's toolset may have changed, run sync_mcp_tools first.",
    params = "common::api::ListMcpToolsByServerRequest"
)]
#[generate_http_handler]
pub async fn list_mcp_tools_by_server(
    ctx: RequestContext,
    params: ListMcpToolsByServerRequest,
) -> Result<ListMcpToolsByServerResponse> {
    let result = domain()
        .mcp_tool_manage()
        .list_mcp_tools_by_server(ctx.clone(), params)
        .await?;

    let ready = probe_runtime_ready(&ctx, &result.items).await;
    let tools: Vec<ToolListItem> = result
        .items
        .iter()
        .map(|t| {
            let runtime_ready = ready.get(&t.po.id).cloned().unwrap_or_default();
            to_list_item(t, runtime_ready)
        })
        .collect();
    Ok(ListMcpToolsByServerResponse {
        tools,
        total: result.total,
    })
}
