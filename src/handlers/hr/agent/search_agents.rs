//! Handler: POST /api/v1/hr/agents/search - Search agents with full filtering
//!
//! 与 query_agents 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, PagedResult, SearchAgentsRequest};
use common::enums::AgentRuntimeState;
use common::enums::AgentStatus;
use common::error::Result;

/// Search AI agents with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_agents",
    name = "search_agents",
    description = "Search agents by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchAgentsRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn search_agents(
    ctx: RequestContext,
    params: SearchAgentsRequest,
) -> Result<PagedResult<AgentListItem>> {
    let search = AgentSearch {
        keyword: params.keyword,
        filters: AgentQuery {
            status: params.status,
            exclude_status: Some(AgentStatus::Deleted),
            created_by: params.created_by,
            model_provider_id: params.model_provider_id,
            roles: params.roles,
            runtime_state: params.runtime_state,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().agent_manage().search_agents(ctx, search).await?;

    Ok(page.map(|agent| {
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
    }))
}
