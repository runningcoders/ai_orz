//! Handler: POST /api/v1/agents/{agent_id}/skills/{skill_id} - Install Skill to Agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{InstallSkillToAgentRequest, InstallSkillToAgentResponse};

use super::response::to_detail;
use common::bail_err;

/// Install an existing public skill to your agent. Creates a private copy of the skill for your agent.
#[register_handler_tool(
    id = "install_skill_to_agent",
    name = "install_skill_to_agent",
    description = "Install an existing public skill to your agent. Creates a private copy of the skill for your agent.",
    params = "common::api::InstallSkillToAgentRequest"
)]
#[generate_http_handler]
pub async fn install_skill_to_agent(
    ctx: RequestContext,
    params: InstallSkillToAgentRequest,
) -> Result<InstallSkillToAgentResponse> {
    let skill = domain()
        .skill_manage()
        .install_to_agent(ctx, &params.skill_id, &params.agent_id)
        .await?;

    Ok(InstallSkillToAgentResponse {
        agent_id: params.agent_id,
        source_skill_id: params.skill_id,
        skill: to_detail(&skill),
    })
}
