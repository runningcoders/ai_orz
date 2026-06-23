//! Handler: GET /api/v1/finance/mcp-servers/{id} - Get MCP Server detail.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetMcpServerRequest, GetMcpServerResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

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
) -> Result<GetMcpServerResponse, AppError> {
    let server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx, &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("McpServer {} not found", params.id)))?;

    Ok(to_detail(&server))
}
