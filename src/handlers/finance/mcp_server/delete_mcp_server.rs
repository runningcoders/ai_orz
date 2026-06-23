//! Handler: DELETE /api/v1/finance/mcp-servers/{id} - Delete MCP Server.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteMcpServerRequest, DeleteMcpServerResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Soft-delete an MCP Server by ID.
#[register_handler_tool(
    id = "delete_mcp_server",
    name = "delete_mcp_server",
    description = "Soft-delete an MCP Server by ID",
    params = "common::api::DeleteMcpServerRequest"
)]
#[generate_http_handler]
pub async fn delete_mcp_server(
    ctx: RequestContext,
    params: DeleteMcpServerRequest,
) -> Result<DeleteMcpServerResponse, AppError> {
    domain()
        .mcp_server_manage()
        .delete_mcp_server(ctx, &params.id)
        .await?;

    Ok(DeleteMcpServerResponse { success: true })
}
