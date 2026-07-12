//! Handler: POST /api/v1/agents/{agent_id}/skill-packs/{tag} - Install skill pack to agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{InstallSkillPackRequest, InstallSkillPackResponse};

/// Install a skill pack (by tag) to an agent.
///
/// Queries all Published skills carrying the given tag and installs them
/// as Draft copies into the Agent's skill directory. Records the tag in
/// the Agent's runtime_config.installed_skill_packs for wake-time injection.
/// Idempotent: if the tag is already installed, no change is made and 0 is returned.
#[generate_http_handler]
pub async fn install_skill_pack(
    ctx: RequestContext,
    params: InstallSkillPackRequest,
) -> Result<InstallSkillPackResponse> {
    let installed_count = domain()
        .agent_manage()
        .install_skill_pack(ctx, &params.agent_id, &params.tag)
        .await?;

    Ok(InstallSkillPackResponse { installed_count })
}
