//! Handler: DELETE /api/v1/agents/{agent_id}/tools/{tool_id}/bind - Unbind a tool from an agent

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UnbindToolFromAgentRequest, UnbindToolFromAgentResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Unbind a tool from an agent so the agent can no longer use it
#[register_handler_tool(
    id = "unbind_tool_from_agent",
    name = "unbind_tool_from_agent",
    description = "Unbind a tool from an agent so the agent can no longer use it",
    params = "common::api::UnbindToolFromAgentRequest",
)]
#[generate_http_handler]
pub async fn unbind_tool_from_agent(
    ctx: RequestContext,
    params: UnbindToolFromAgentRequest,
) -> Result<UnbindToolFromAgentResponse, AppError> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.tool_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.tool_id)))?;

    domain()
        .tool_provider_manage()
        .unbind_tool_from_agent(ctx, &params.agent_id, &params.tool_id)
        .await?;

    Ok(UnbindToolFromAgentResponse { success: true })
}