//! Handler: GET /api/v1/hr/agents - List all agents with optional status filtering

use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, ListAgentsRequest, PagedResult};
use common::enums::AgentRuntimeState;
use common::enums::AgentStatus;
use common::error::Result;

/// List all AI agents with optional status filtering
#[register_handler_tool(
    id = "list_agents",
    name = "List All Agents",
    description = "Browse all agents with pagination (Deleted ones excluded); each item carries id, name, roles, status, and runtime state. Use this when you have no specific filter in mind. For exact-field filtering use query_agents; for free-text relevance ranking use search_agents.",
    params = "common::api::ListAgentsRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,
) -> Result<PagedResult<AgentListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 Deleted
    let page = domain()
        .agent_manage()
        .query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|agent| {
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
            kind: agent.po.kind.to_string(),
            model_provider_id: agent.po.model_provider_id.clone(),
            status: agent.po.status as i32,
            created_at: agent.po.created_at,
            runtime_state,
        }
    }))
}
