//! Handler: POST /api/v1/agents/{agent_id}/tools/{tool_id}/bind - Bind a tool to an agent

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{BindToolToAgentRequest, BindToolToAgentResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Bind an existing tool to an agent so the agent can use it for tool calling
#[register_handler_tool(
    id = "bind_tool_to_agent",
    name = "bind_tool_to_agent",
    description = "Bind an existing tool to an agent so the agent can use it for tool calling",
    params = "common::api::BindToolToAgentRequest",
)]
#[generate_http_handler]
pub async fn bind_tool_to_agent(
    ctx: RequestContext,
    params: BindToolToAgentRequest,
) -> Result<BindToolToAgentResponse, AppError> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.tool_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.tool_id)))?;

    domain()
        .tool_provider_manage()
        .bind_tool_to_agent(ctx, &params.agent_id, &params.tool_id)
        .await?;

    Ok(BindToolToAgentResponse { success: true })
}