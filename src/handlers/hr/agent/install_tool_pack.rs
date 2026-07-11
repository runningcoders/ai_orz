//! Handler: POST /api/v1/agents/{agent_id}/tool-packs/{tag} - Install tool pack to agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{InstallToolPackRequest, InstallToolPackResponse};

/// Install a tool pack (by tag) to an agent.
///
/// Adds the tag to the agent's runtime_config.installed_tags, enabling
/// wake-time injection of all tools carrying that tag (no per-tool binding required).
/// Idempotent: if the tag is already installed, no change is made.
#[generate_http_handler]
pub async fn install_tool_pack(
    ctx: RequestContext,
    params: InstallToolPackRequest,
) -> Result<InstallToolPackResponse> {
    domain()
        .agent_manage()
        .install_tool_pack(ctx.clone(), &params.agent_id, &params.tag)
        .await?;

    let installed_tags = domain()
        .agent_manage()
        .list_installed_tool_packs(ctx, &params.agent_id)
        .await?;

    Ok(InstallToolPackResponse {
        agent_id: params.agent_id,
        installed_tags,
    })
}
