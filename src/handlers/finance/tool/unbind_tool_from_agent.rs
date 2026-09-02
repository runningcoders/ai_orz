//! Handler: DELETE /api/v1/agents/{agent_id}/tools/{tool_id}/bind - Unbind a tool from an agent

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UnbindToolFromAgentRequest, UnbindToolFromAgentResponse};
use common::error::Result;

/// Unbind a tool from an agent so the agent can no longer use it
#[register_handler_tool(
    id = "unbind_tool_from_agent",
    name = "Unbind Tool from Agent",
    description = "Remove a tool binding from an agent so the agent can no longer call that tool. Returns success:true; fails if the tool does not exist. Use bind_tool_to_agent to grant access.",
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
