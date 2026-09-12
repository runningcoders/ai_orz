//! Handler: POST /api/v1/agents/{id}/career - 职业选择（初创 → 面试中）
//!
//! 语义化动作：按 Agent 的 `roles ∪ capabilities` 匹配并安装个人工具包/技能包，
//! 完成后才允许进入面试环节 —— 对应「先学完职业技能，再去面试」。
//!
//! 真正的绑定逻辑在 domain 的 `Incubating → Interviewing` 这条边上，
//! 本 handler 只负责定位 Agent 并驱动状态流转。

use crate::handlers::hr::agent::build_status_response;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SelectAgentCareerRequest, UpdateAgentStatusResponse};
use common::enums::AgentStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Complete an agent's career selection (Incubating -> Interviewing).
///
/// Installs the tool/skill packs matching the agent's own roles and capabilities.
#[register_handler_tool(
    id = "select_agent_career",
    name = "Select Agent Career",
    description = "Complete an agent's career selection: move it from Incubating to Interviewing and install the tool/skill packs matching its roles and capabilities. The agent must be in Incubating status.",
    params = "common::api::SelectAgentCareerRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn select_agent_career(
    ctx: RequestContext,
    params: SelectAgentCareerRequest,
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
        .transition_status(ctx, &mut agent, AgentStatus::Interviewing, None)
        .await?;

    Ok(build_status_response(&agent))
}
