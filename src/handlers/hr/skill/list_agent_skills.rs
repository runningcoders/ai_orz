//! Handler: GET /api/v1/agents/{agent_id}/skills - List all skills installed in an agent

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListAgentSkillsRequest, ListAgentSkillsResponse, SkillListItem};

use super::response::to_list_item;

/// List all skills that are currently installed in the specified agent. Returns basic information for each skill.
#[register_handler_tool(
    id = "list_agent_skills",
    name = "list_agent_skills",
    description = "List all skills that are currently installed in the specified agent. Returns basic information for each skill.",
    params = "common::api::ListAgentSkillsRequest"
)]
#[generate_http_handler]
pub async fn list_agent_skills(
    ctx: RequestContext,
    params: ListAgentSkillsRequest,
) -> Result<ListAgentSkillsResponse, AppError> {
    let skills = domain()
        .skill_manage()
        .list_for_agent(ctx, &params.agent_id)
        .await?;

    let skills = skills.iter().map(to_list_item).collect();
    Ok(ListAgentSkillsResponse { skills })
}
