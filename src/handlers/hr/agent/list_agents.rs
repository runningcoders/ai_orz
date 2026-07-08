//! Handler: GET /api/v1/agents - List all agents with optional status filtering

use common::enums::AgentRuntimeState;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, ListAgentsRequest, ListAgentsResponse};

/// List all AI agents with optional status filtering
#[register_handler_tool(
    id = "list_agents",
    name = "list_agents",
    description = "List all AI agents with optional status filtering",
    params = "common::api::ListAgentsRequest"
)]
#[generate_http_handler]
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,
) -> Result<ListAgentsResponse> {
    let agents = domain().agent_manage().list_agents(ctx).await?;
    let agents: Vec<AgentListItem> = agents
        .iter()
        .filter(|agent| {
            if let Some(status) = params.status {
                agent.po.status == status
            } else {
                true
            }
        })
        .map(|agent| {
            // 从 runtime_info 读取运行时状态
            let runtime_state = match &agent.runtime_info {
                Some(info) => info.state as i32,
                None => AgentRuntimeState::Idle as i32,
            };

            AgentListItem {
                id: agent.id().to_string(),
                name: agent.name().to_string(),
                roles: agent.po.get_roles(),
                description: if agent.po.description.is_empty() {
                    None
                } else {
                    Some(agent.po.description.clone())
                },
                model_provider_id: agent.po.model_provider_id.clone(),
                status: agent.po.status as i32,
                created_at: agent.po.created_at,
                runtime_state,
            }
        })
        .collect();

    Ok(ListAgentsResponse { agents })
}
