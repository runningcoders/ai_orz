//! Handler: POST /api/v1/hr/agents/query - Agent 通用查询接口
//!
//! 与 list_agents 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, AgentQueryRequest, PagedResult};
use common::enums::AgentRuntimeState;
use common::enums::AgentStatus;
use common::error::Result;

/// Agent 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_agents",
    name = "Query Agents (Advanced)",
    description = "Query agents with full filtering support (ids, keyword, status, roles)",
    params = "common::api::AgentQueryRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn query_agents(
    ctx: RequestContext,
    params: AgentQueryRequest,
) -> Result<PagedResult<AgentListItem>> {
    let page = domain()
        .agent_manage()
        .query(
            ctx,
            AgentQuery {
                ids: params.ids,
                keyword: params.keyword,
                status: params.status,
                exclude_status: Some(AgentStatus::Deleted),
                created_by: params.created_by,
                model_provider_id: params.model_provider_id,
                roles: params.roles,
                runtime_state: params.runtime_state,
                pagination: params.pagination,
            },
        )
        .await?;

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
