//! Handler: GET /api/v1/agents/{agent_id}/skill-packs - List installed skill packs

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListInstalledSkillPacksRequest, ListSkillPacksResponse};
use common::error::Result;

/// List all skill pack tags installed on an agent.
#[register_handler_tool(
    id = "list_installed_skill_packs",
    name = "List Agent's Skill Packs",
    description = "List the skill pack tags currently installed on an agent. Use it to check which packs an agent already has before installing or uninstalling one.",
    params = "common::api::ListInstalledSkillPacksRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn list_installed_skill_packs(
    ctx: RequestContext,
    params: ListInstalledSkillPacksRequest,
) -> Result<ListSkillPacksResponse> {
    let skill_packs = domain()
        .agent_manage()
        .list_installed_skill_packs(ctx, &params.agent_id)
        .await?;

    Ok(ListSkillPacksResponse { skill_packs })
}
