//! Handler: GET /api/v1/agents/{agent_id}/skill-packs - List installed skill packs

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListInstalledSkillPacksRequest, ListSkillPacksResponse};

/// List all skill pack tags installed on an agent.
#[register_handler_tool(
    id = "list_installed_skill_packs",
    name = "list_installed_skill_packs",
    description = "List all skill pack tags installed on an agent. Returns the tags recorded in runtime_config.installed_skill_packs.",
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
