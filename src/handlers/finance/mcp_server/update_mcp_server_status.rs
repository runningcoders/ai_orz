//! Handler: PUT /api/v1/finance/mcp-servers/{id}/status - Update MCP Server status.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateMcpServerStatusRequest, UpdateMcpServerStatusResponse};

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::{to_detail, to_model_status};
use common::error::{Result, err};

/// Update an MCP Server status. Use DELETE for soft deletion.
#[register_handler_tool(
    id = "update_mcp_server_status",
    name = "Toggle MCP Server Status",
    description = "Update an MCP Server status. Use DELETE for soft deletion",
    params = "common::api::UpdateMcpServerStatusRequest"
)]
#[generate_http_handler]
pub async fn update_mcp_server_status(
    ctx: RequestContext,
    params: UpdateMcpServerStatusRequest,
) -> Result<UpdateMcpServerStatusResponse> {
    domain()
        .mcp_server_manage()
        .update_mcp_server_status(ctx.clone(), &params.id, to_model_status(params.status))
        .await?;

    let server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "McpServer {} not found", params.id))?;

    Ok(to_detail(&server))
}
