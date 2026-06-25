//! Handler: POST /api/v1/agents/{agent_id}/tools/{tool_id}/bind - Bind a tool to an agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{BindToolToAgentRequest, BindToolToAgentResponse};
use common::bail_err;

/// Bind an existing tool to an agent so the agent can use it for tool calling
#[register_handler_tool(
    id = "bind_tool_to_agent",
    name = "bind_tool_to_agent",
    description = "Bind an existing tool to an agent so the agent can use it for tool calling",
    params = "common::api::BindToolToAgentRequest"
)]
#[generate_http_handler]
pub async fn bind_tool_to_agent(
    ctx: RequestContext,
    params: BindToolToAgentRequest,
) -> Result<BindToolToAgentResponse> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.tool_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Tool {} not found", params.tool_id)))?;

    domain()
        .tool_provider_manage()
        .bind_tool_to_agent(ctx, &params.agent_id, &params.tool_id)
        .await?;

    Ok(BindToolToAgentResponse { success: true })
}
