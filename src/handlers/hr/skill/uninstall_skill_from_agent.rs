//! Handler: DELETE /api/v1/hr/agents/{agent_id}/skills/{skill_id} - 从 Agent 卸载技能副本

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UninstallSkillFromAgentRequest, UninstallSkillFromAgentResponse};
use common::error::Result;

/// 从 Agent 目录卸载技能副本。删除 Agent 的私有副本（DB 记录 + 文件目录）。
/// 仅适用于通过 install_skill_to_agent 安装的副本（parent_skill_id 不为空）。
#[register_handler_tool(
    id = "uninstall_skill_from_agent",
    name = "uninstall_skill_from_agent",
    description = "Uninstall a skill copy from an agent. Deletes the agent's private copy.",
    params = "common::api::UninstallSkillFromAgentRequest",
    tags = "skill_management",
    neural
)]
#[generate_http_handler]
pub async fn uninstall_skill_from_agent(
    ctx: RequestContext,
    params: UninstallSkillFromAgentRequest,
) -> Result<UninstallSkillFromAgentResponse> {
    let ctx = ctx.to_builder().agent_id(&params.agent_id).build();
    domain()
        .skill_manage()
        .uninstall_from_agent(ctx, &params.skill_id, &params.agent_id)
        .await?;
    Ok(UninstallSkillFromAgentResponse {
        agent_id: params.agent_id,
        skill_id: params.skill_id,
        deleted: true,
    })
}
