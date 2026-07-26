//! Handler: DELETE /api/v1/agents/{agent_id}/tools/{tool_id}/bind - Unbind a tool from an agent

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UnbindToolFromAgentRequest, UnbindToolFromAgentResponse};
use common::error::Result;

/// Unbind a tool from an agent so the agent can no longer use it
#[register_handler_tool(
    id = "unbind_tool_from_agent",
    name = "unbind_tool_from_agent",
    description = "Unbind a tool from an agent so the agent can no longer use it",
    params = "common::api::UnbindToolFromAgentRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn unbind_tool_from_agent(
    ctx: RequestContext,
    params: UnbindToolFromAgentRequest,
) -> Result<UnbindToolFromAgentResponse> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.tool_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Tool {} not found", params.tool_id))
        })?;

    let ctx = ctx.to_builder().agent_id(&params.agent_id).build();

    domain()
        .tool_provider_manage()
        .unbind_tool_from_agent(ctx, &params.agent_id, &params.tool_id)
        .await?;

    Ok(UnbindToolFromAgentResponse { success: true })
}
