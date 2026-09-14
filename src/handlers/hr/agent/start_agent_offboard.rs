//! Handler: POST /api/v1/agents/{id}/offboard/start - 发起离职（已入职 → 待离职）
//!
//! 语义化动作：Agent 进入交接期 —— **不再接受新业务**（候选路由只匹配
//! Onboarded，见 `resolve_scored`），已在运行的业务继续执行直至完成，
//! 随后由「完成离职」（complete_agent_offboard）收尾。

use crate::handlers::hr::agent::build_status_response;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{StartAgentOffboardRequest, UpdateAgentStatusResponse};
use common::enums::AgentStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Start offboarding an agent (Onboarded -> PendingOffboard).
///
/// The agent enters a handover period: it stops receiving new business
/// immediately (candidate routing only matches Onboarded agents), while
/// already-running business keeps running to completion. Finish the flow
/// with complete_agent_offboard once the handover is done.
#[register_handler_tool(
    id = "start_agent_offboard",
    name = "Start Agent Offboard",
    description = "Start offboarding an agent: move it from Onboarded to PendingOffboard (handover period). It stops receiving new business immediately while already-running business keeps finishing. The agent must be in Onboarded status.",
    params = "common::api::StartAgentOffboardRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn start_agent_offboard(
    ctx: RequestContext,
    params: StartAgentOffboardRequest,
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
        .transition_status(ctx, &mut agent, AgentStatus::PendingOffboard, None)
        .await?;

    Ok(build_status_response(&agent))
}
