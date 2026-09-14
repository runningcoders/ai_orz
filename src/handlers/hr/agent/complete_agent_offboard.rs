//! Handler: POST /api/v1/agents/{id}/offboard/complete - 完成离职（待离职 → 已离职）
//!
//! 语义化动作：执行业务交接后正式下线。交接逻辑目前为占位
//! （见 `HrDomainImpl::handover_business`），移交策略设计定稿后填充。

use crate::handlers::hr::agent::build_status_response;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CompleteAgentOffboardRequest, UpdateAgentStatusResponse};
use common::enums::AgentStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Complete offboarding an agent (PendingOffboard -> Offboarded).
///
/// Runs the business handover (currently a placeholder) and takes the agent
/// offline for good. After this, the agent record can be safely deleted.
#[register_handler_tool(
    id = "complete_agent_offboard",
    name = "Complete Agent Offboard",
    description = "Complete offboarding an agent: move it from PendingOffboard to Offboarded after running the business handover. The agent goes permanently offline and its record can be safely deleted afterwards. The agent must be in PendingOffboard status.",
    params = "common::api::CompleteAgentOffboardRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn complete_agent_offboard(
    ctx: RequestContext,
    params: CompleteAgentOffboardRequest,
) -> Result<UpdateAgentStatusResponse> {
    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, Default::default())
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);
    let mut agent = agent;

    domain()
        .agent_manage()
        .transition_status(ctx, &mut agent, AgentStatus::Offboarded, None)
        .await?;

    Ok(build_status_response(&agent))
}
