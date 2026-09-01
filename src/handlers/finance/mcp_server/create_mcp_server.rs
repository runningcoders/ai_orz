//! Handler: POST /api/v1/finance/mcp-servers - Create a new MCP Server.

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateMcpServerRequest, CreateMcpServerResponse};
use uuid::Uuid;

use crate::models::mcp_server::McpServer;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::Result;

use super::response::{to_detail, to_model_config, to_model_transport};

/// Create a new MCP Server configuration for MCP tool discovery and invocation.
#[register_handler_tool(
    id = "create_mcp_server",
    name = "Add MCP Server",
    description = "Create a new MCP Server configuration for MCP tool discovery and invocation",
    params = "common::api::CreateMcpServerRequest"
)]
#[generate_http_handler]
pub async fn create_mcp_server(
    ctx: RequestContext,
    params: CreateMcpServerRequest,
) -> Result<CreateMcpServerResponse> {
    let transport = to_model_transport(params.transport);
    let config = to_model_config(params.config);
    // 配置期校验：binding ↔ 协议 / platform ↔ kind / 三元组去重（单点 pkg::credential）
    crate::pkg::credential::validate_requirements(
        &config.credential_requirements,
        common::models::mcp_transport_scope(params.transport),
    )?;
    let server = McpServer::new(
        Uuid::now_v7().to_string(),
        params.name,
        transport,
        config,
        Some(ctx.uid()),
    );
    let server_id = server.po.id.clone();

    domain()
        .mcp_server_manage()
        .create_mcp_server(ctx.clone(), &server)
        .await?;

    let server = domain()
        .mcp_server_manage()
        .get_mcp_server(ctx, &server_id)
        .await?
        .unwrap_or_else(|| server.redacted_for_management());

    Ok(to_detail(&server))
}
