//! Handler: GET /api/v1/hr/agents - List all agents with optional status filtering
//!
//! 档案字段走 `hr.agent_profiles` 缓存（写路径主动失效），运行时状态每请求
//! 从 runtime 域实时合并 —— 缓存里不含秒级变化的字段。

use std::collections::HashMap;

use crate::pkg::RequestContext;
use crate::service::domain::hr::agent_profile;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, ListAgentsRequest, PagedResult};
use common::enums::AgentRuntimeState;
use common::error::Result;

/// List all AI agents with optional status filtering
#[register_handler_tool(
    id = "list_agents",
    name = "List All Agents",
    description = "Browse all agents with pagination (Deleted ones excluded); each item carries id, name, roles, status, and runtime state. Use this when you have no specific filter in mind. For exact-field filtering use query_agents; for free-text relevance ranking use search_agents.",
    params = "common::api::ListAgentsRequest",
    tags = "collaboration,agent_management,hr_specialist,reception"
)]
#[generate_http_handler]
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,
) -> Result<PagedResult<AgentListItem>> {
    let profiles = agent_profile::profiles(&ctx).await?;

    // 运行时状态是内存态，始终实时读取；档案不缓存它
    let runtime: HashMap<String, i32> = runtime_domain()
        .list_runtime_agents(None, None, None)
        .into_iter()
        .map(|(id, info)| (id, info.state as i32))
        .collect();

    let total = profiles.len();
    let offset = params.pagination.offset.unwrap_or(0);
    let items: Vec<AgentListItem> = profiles
        .into_iter()
        .skip(offset)
        .take(params.pagination.limit.unwrap_or(usize::MAX))
        .map(|profile| AgentListItem {
            runtime_state: runtime
                .get(&profile.id)
                .copied()
                .unwrap_or(AgentRuntimeState::Idle as i32),
            id: profile.id,
            name: profile.name,
            roles: profile.roles,
            description: profile.description,
            kind: profile.kind,
            model_provider_id: profile.model_provider_id,
            status: profile.status,
            created_at: profile.created_at,
        })
        .collect();

    Ok(PagedResult { items, total })
}
