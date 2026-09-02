//! Handler: GET /api/v1/agents/{agent_id}/skills/expired
//!
//! 返回 Agent 名下**仅 Expired** 的技能副本（与 GET /agents/{agent_id}/skills 天然互斥），
//! 用于详情页「📦 已过期技能」虚拟 pack 点击时的异步加载数据源。

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListExpiredAgentSkillsRequest, ListExpiredAgentSkillsResponse};
use common::error::Result;

use super::response::to_list_item;

/// List all Expired-only skills currently under the specified agent's private directory.
/// Mirrors list_agent_skills but filters only for SkillStatus::Expired.
#[register_handler_tool(
    id = "list_expired_agent_skills",
    name = "List Agent's Expired Skills",
    description = "List only Expired-status skill copies under the specified agent — the complement of list_agent_skills, which excludes them. Useful to find candidates before calling restore_skill.",
    params = "common::api::ListExpiredAgentSkillsRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn list_expired_agent_skills(
    ctx: RequestContext,
    params: ListExpiredAgentSkillsRequest,
) -> Result<ListExpiredAgentSkillsResponse> {
    let ctx = ctx.to_builder().agent_id(&params.agent_id).build();
    let skills = domain()
        .skill_manage()
        .list_expired_for_agent(ctx, &params.agent_id)
        .await?;

    let skills = skills.iter().map(to_list_item).collect();
    Ok(ListExpiredAgentSkillsResponse { skills })
}
