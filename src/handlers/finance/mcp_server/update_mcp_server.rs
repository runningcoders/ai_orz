//! Handler: PUT /api/v1/finance/mcp-servers/{id} - Update MCP Server.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateMcpServerRequest, UpdateMcpServerResponse};

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::Result;

use super::response::{to_api_transport, to_detail, to_model_config, to_model_transport};

/// Update an MCP Server configuration.
#[register_handler_tool(
    id = "update_mcp_server",
    name = "Update MCP Server",
    description = "Update an MCP Server's name, transport, or connection config; only provided fields change and credentials stay redacted. Returns the updated detail; fails if the server does not exist or the credential requirements are invalid for the transport.",
    params = "common::api::UpdateMcpServerRequest"
)]
#[generate_http_handler]
pub async fn update_mcp_server(
    ctx: RequestContext,
    params: UpdateMcpServerRequest,
) -> Result<UpdateMcpServerResponse> {
    let mut server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("McpServer {} not found", params.id))
        })?;

    if let Some(name) = params.name {
        server.po.name = name;
    }
    if let Some(transport) = params.transport {
        server.po.transport = to_model_transport(transport);
    }
    if let Some(config) = params.config {
        let model_config = to_model_config(config);
        // 配置期校验（作用域取更新后生效的 transport）
        crate::pkg::credential::validate_requirements(
            &model_config.credential_requirements,
            common::models::mcp_transport_scope(to_api_transport(server.po.transport)),
        )?;
        server.po.set_config(&model_config);
    }

    let server_id = server.po.id.clone();
    domain()
        .mcp_server_manage()
        .update_mcp_server(ctx.clone(), &server)
        .await?;

    let server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx, &server_id)
        .await?
        .unwrap_or_else(|| server.redacted_for_management());

    Ok(to_detail(&server))
}
