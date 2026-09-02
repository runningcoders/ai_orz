//! Handler: GET /api/v1/agents/{agent_id}/tool-packs - List installed tool packs

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListInstalledToolPacksRequest, ListInstalledToolPacksResponse};
use common::error::Result;

/// List all tool pack tags installed on an agent.
#[register_handler_tool(
    id = "list_installed_tool_packs",
    name = "List Agent's Tool Packs",
    description = "List the tool pack tags currently installed on an agent (each tag auto-injects its tools at wake time). Use it to verify an agent's packs before install or uninstall operations.",
    params = "common::api::ListInstalledToolPacksRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn list_installed_tool_packs(
    ctx: RequestContext,
    params: ListInstalledToolPacksRequest,
) -> Result<ListInstalledToolPacksResponse> {
    let installed_tags = domain()
        .agent_manage()
        .list_installed_tool_packs(ctx, &params.agent_id)
        .await?;

    Ok(ListInstalledToolPacksResponse {
        agent_id: params.agent_id,
        installed_tags,
    })
}
