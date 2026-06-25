//! Handler: GET /api/v1/finance/mcp-servers/{id} - Get MCP Server detail.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetMcpServerRequest, GetMcpServerResponse};

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::bail_err;

/// Get a management-safe MCP Server detail by ID.
#[register_handler_tool(
    id = "get_mcp_server",
    name = "get_mcp_server",
    description = "Get a management-safe MCP Server detail by ID",
    params = "common::api::GetMcpServerRequest"
)]
#[generate_http_handler]
pub async fn get_mcp_server(
    ctx: RequestContext,
    params: GetMcpServerRequest,
) -> Result<GetMcpServerResponse> {
    let server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx, &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("McpServer {} not found", params.id)))?;

    Ok(to_detail(&server))
}
