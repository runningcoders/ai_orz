//! Handler: POST /api/v1/agents/{id}/onboard - 入职（待入职 → 已入职）
//!
//! 语义化动作：**真正的入职流程在这条边内完成** —— 安装组织要求的工具包/技能包，
//! 而不是入职之后再补装。
//!
//! `packs` 不传时回退组织级配置 `OrganizationConfig.agent_onboard`；
//! 传入则以本次为准（组织配置不再叠加）。

use crate::handlers::hr::agent::build_status_response;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{OnboardAgentRequest, UpdateAgentStatusResponse};
use common::enums::AgentStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Onboard an agent (PendingOnboard -> Onboarded), installing the organization's required packs.
///
/// The installation happens as part of this transition, not after it.
/// When `packs` is omitted, the organization-level config is used.
#[register_handler_tool(
    id = "onboard_agent",
    name = "Onboard Agent",
    description = "Onboard an agent: move it from PendingOnboard to Onboarded and install the tool/skill packs the organization requires. Pass packs to override the organization defaults. The agent must be in PendingOnboard status.",
    params = "common::api::OnboardAgentRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn onboard_agent(
    ctx: RequestContext,
    params: OnboardAgentRequest,
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
        .transition_status(ctx, &mut agent, AgentStatus::Onboarded, params.packs)
        .await?;

    Ok(build_status_response(&agent))
}
