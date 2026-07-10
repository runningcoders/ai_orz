//! Handler: GET /api/v1/agents/{id} - Get agent detailed information

use common::enums::AgentRuntimeState;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetAgentRequest, GetAgentResponse};

/// Get detailed information about an AI agent
#[register_handler_tool(
    id = "get_agent",
    name = "get_agent",
    description = "Get detailed information about an AI agent by ID",
    params = "common::api::GetAgentRequest"
)]
#[generate_http_handler]
pub async fn get_agent(
    ctx: RequestContext,
    params: GetAgentRequest,
) -> Result<GetAgentResponse> {
    let agent = domain()
        .agent_manage()
        .get_agent(ctx, &params.id, Default::default())
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let capabilities: Vec<String> = agent.po.get_capabilities();
    let roles: Vec<String> = agent.po.get_roles();

    // 从 runtime_info 读取运行时状态
    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };

    Ok(GetAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles,
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
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
        runtime_state,
        current_message_id,
    })
}
