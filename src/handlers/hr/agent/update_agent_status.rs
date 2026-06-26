//! Handler: PUT /api/v1/agents/{id}/status - Update agent status

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateAgentStatusRequest, UpdateAgentStatusResponse};

/// Update the status of an AI agent (active/disabled)
#[register_handler_tool(
    id = "update_agent_status",
    name = "update_agent_status",
    description = "Update the status of an AI agent (active/disabled)",
    params = "common::api::UpdateAgentStatusRequest"
)]
#[generate_http_handler]
pub async fn update_agent_status(
    ctx: RequestContext,
    params: UpdateAgentStatusRequest,
) -> Result<UpdateAgentStatusResponse> {
    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    domain()
        .agent_manage()
        .transition_status(ctx, &mut agent, params.status)
        .await?;

    let capabilities: Vec<String> = agent.po.get_capabilities();

    Ok(UpdateAgentStatusResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles: agent.po.get_roles(),
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: {
            let capabilities = agent.po.get_capabilities();
            if capabilities.is_empty() {
                None
            } else {
                Some(capabilities)
            }
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        model_provider_id: agent.po.model_provider_id.clone(),
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
    })
}
