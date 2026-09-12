//! Handler: PUT /api/v1/agents/{id}/status - 通用状态流转
//!
//! 语义：**只搬状态，不做业务**。带副作用的流转请走语义化接口：
//! - 职业选择（初创 → 面试中）：`POST /agents/{id}/career`
//! - 入职（待入职 → 已入职）：`POST /agents/{id}/onboard`
//!
//! 本接口保留给「无副作用的边」（如 面试中 → 待入职）与幂等的状态纠正，
//! 前端的状态切换按钮统一走它，由 domain 层按边决定是否触发副作用。

use crate::handlers::hr::agent::build_status_response;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateAgentStatusRequest, UpdateAgentStatusResponse};
use common::error::Result;

use crate::enrich_ctx;

/// Transition an agent's lifecycle status.
///
/// Statuses: Incubating, Interviewing, PendingOnboard, Onboarded, PendingOffboard, Offboarded.
/// Side-effect-free transitions only — use the semantic endpoints
/// (`career` / `onboard`) when capabilities must be installed.
#[register_handler_tool(
    id = "update_agent_status",
    name = "Toggle Agent Status",
    description = "Transition an agent's lifecycle status (Incubating, Interviewing, PendingOnboard, Onboarded, PendingOffboard, Offboarded) and return the updated agent. Pure status move without capability binding; for career selection use select_agent_career, for onboarding use onboard_agent.",
    params = "common::api::UpdateAgentStatusRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn update_agent_status(
    ctx: RequestContext,
    params: UpdateAgentStatusRequest,
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
        .transition_status(ctx, &mut agent, params.status, params.packs)
        .await?;

    Ok(build_status_response(&agent))
}
