//! Handler: DELETE /api/v1/agents/{agent_id}/tool-packs/{tag} - Uninstall tool pack from agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UninstallToolPackRequest, UninstallToolPackResponse};

/// Uninstall a tool pack (by tag) from an agent.
///
/// Removes the tag from the agent's runtime_config.installed_tags.
/// Tools carrying that tag will no longer be auto-injected at wake time.
/// Idempotent: if the tag is not installed, no change is made.
#[register_handler_tool(
    id = "uninstall_tool_pack",
    name = "uninstall_tool_pack",
    description = "Uninstall a tool pack (by tag) from an agent. Removes the tag from runtime_config.installed_tags. Tools carrying that tag will no longer be auto-injected at wake time. Idempotent.",
    params = "common::api::UninstallToolPackRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn uninstall_tool_pack(
    ctx: RequestContext,
    params: UninstallToolPackRequest,
) -> Result<UninstallToolPackResponse> {
    domain()
        .agent_manage()
        .uninstall_tool_pack(ctx.clone(), &params.agent_id, &params.tag)
        .await?;

    let installed_tags = domain()
        .agent_manage()
        .list_installed_tool_packs(ctx, &params.agent_id)
        .await?;

    Ok(UninstallToolPackResponse {
        agent_id: params.agent_id,
        installed_tags,
    })
}
