//! Handler: GET /api/v1/agents/search - Search agents by keyword

use common::enums::AgentRuntimeState;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, SearchAgentsRequest, SearchAgentsResponse};

/// Search AI agents by keyword (FTS5 full-text search + vector semantic search)
#[register_handler_tool(
    id = "search_agents",
    name = "search_agents",
    description = "Search AI agents by keyword. Supports FTS5 full-text search and vector semantic search.",
    params = "common::api::SearchAgentsRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn search_agents(
    ctx: RequestContext,
    params: SearchAgentsRequest,
) -> Result<SearchAgentsResponse> {
    let search = AgentSearch {
        keyword: params.keyword,
        filters: AgentQuery {
            pagination: common::api::PaginationParams { limit: params.limit, offset: None },
            ..Default::default()
        },
        ..Default::default()
    };

    let agents = domain().agent_manage().search_agents(ctx, search).await?;
    let agents: Vec<AgentListItem> = agents
        .iter()
        .map(|agent| {
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
                kind: agent.po.kind.to_string(),
                model_provider_id: agent.po.model_provider_id.clone(),
                status: agent.po.status as i32,
                created_at: agent.po.created_at,
                runtime_state,
            }
        })
        .collect();

    Ok(SearchAgentsResponse { agents })
}