//! Handler: GET /api/v1/agents/{agent_id}/skills - List all skills installed in an agent

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListAgentSkillsRequest, ListAgentSkillsResponse};
use common::error::Result;

use super::response::to_list_item;

/// List all skills that are currently installed in the specified agent. Returns basic information for each skill.
#[register_handler_tool(
    id = "list_agent_skills",
    name = "List Agent's Active Skills",
    description = "List all currently active skills installed under the specified agent. Expired copies are excluded — call list_expired_agent_skills for those. Use install_skill_to_agent to add more.",
    params = "common::api::ListAgentSkillsRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn list_agent_skills(
    ctx: RequestContext,
    params: ListAgentSkillsRequest,
) -> Result<ListAgentSkillsResponse> {
    let skills = domain()
        .skill_manage()
        .list_for_agent(ctx, &params.agent_id)
        .await?;

    let skills = skills.iter().map(to_list_item).collect();
    Ok(ListAgentSkillsResponse { skills })
}
