//! Handler: GET /api/v1/finance/mcp-servers - List MCP Servers.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListMcpServersRequest, ListMcpServersResponse, McpServerListItem};

use crate::models::mcp_server::McpServerQuery;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::Result;

use super::response::{to_list_item, to_model_status, to_model_transport};

/// List management-safe MCP Servers with optional filters.
#[register_handler_tool(
    id = "list_mcp_servers",
    name = "list_mcp_servers",
    description = "List management-safe MCP Servers with optional filters",
    params = "common::api::ListMcpServersRequest"
)]
#[generate_http_handler]
pub async fn list_mcp_servers(
    ctx: RequestContext,
    params: ListMcpServersRequest,
) -> Result<ListMcpServersResponse> {
    let query = McpServerQuery {
        id: params.id,
        name: params.name,
        transport: params.transport.map(to_model_transport),
        status: params.status.map(to_model_status),
        pagination: params.pagination,
        ..Default::default()
    };

    let page = domain()
        .mcp_server_manage()
        .query_mcp_servers(ctx, query)
        .await?;

    let servers: Vec<McpServerListItem> = page.items.iter().map(to_list_item).collect();
    Ok(ListMcpServersResponse {
        servers,
        total: page.total,
    })
}
