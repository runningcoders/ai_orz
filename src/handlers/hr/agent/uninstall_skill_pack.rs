//! Handler: DELETE /api/v1/agents/{agent_id}/skill-packs/{tag} - Uninstall skill pack from agent

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UninstallSkillPackRequest, UninstallSkillPackResponse};
use common::error::Result;

/// Uninstall a skill pack (by tag) from an agent.
///
/// Removes the tag from the Agent's runtime_config.installed_skill_packs.
/// When query param `delete_copies=true`, also deletes the Agent's skill copies under that tag.
/// Otherwise already-installed skill copies are preserved (not deleted).
/// Idempotent: if the tag is not installed, no change is made.
#[register_handler_tool(
    id = "uninstall_skill_pack",
    name = "uninstall_skill_pack",
    description = "Uninstall a skill pack (by tag) from an agent. Removes the tag from runtime_config.installed_skill_packs. Pass delete_copies=true to also delete the Agent's skill copies under that tag. Idempotent.",
    params = "common::api::UninstallSkillPackRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn uninstall_skill_pack(
    ctx: RequestContext,
    params: UninstallSkillPackRequest,
) -> Result<UninstallSkillPackResponse> {
    let delete_copies = params.delete_copies.unwrap_or(false);
    domain()
        .agent_manage()
        .uninstall_skill_pack(ctx, &params.agent_id, &params.tag, delete_copies)
        .await?;

    Ok(UninstallSkillPackResponse {})
}
