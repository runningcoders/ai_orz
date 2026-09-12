//! Handler: POST /api/v1/agents/{agent_id}/sync-packs - Sync agent packs

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SyncAgentPacksRequest, SyncAgentPacksResponse};
use common::error::Result;

/// Sync an agent's packs (generic recovery / sync entry).
///
/// Two-phase, idempotent:
/// 1. Install missing base packs (neural / skill_management / tool_management,
///    both tool packs and skill packs);
/// 2. For every installed skill pack, detect newly published skills the agent
///    does not own yet and reinstall that pack to fill the gaps.
#[generate_http_handler]
pub async fn sync_agent_packs(
    ctx: RequestContext,
    params: SyncAgentPacksRequest,
) -> Result<SyncAgentPacksResponse> {
    domain()
        .agent_manage()
        .sync_agent_packs(ctx, &params.agent_id)
        .await
}
