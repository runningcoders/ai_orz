//! Handler: POST /api/v1/agents/{agent_id}/train - Agent 进修

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{TrainAgentRequest, TrainAgentResponse};
use common::error::Result;

/// Agent 进修（在职学习入口）。
///
/// 把 Agent 的能力补齐到其当前职业/组织要求的最新状态，三阶段幂等执行：
/// 1. 补修基础课：安装缺失的基础包（neural / skill_management / tool_management，
///    工具包与技能包两侧）；
/// 2. 学习技能更新：对每个已安装技能包，检测新增已发布技能并重装补全
///    （顺带刷新已有副本内容）；
/// 3. 补学新课：按已走过的状态机边重跑匹配 —— 职业匹配（非初创）与
///    组织要求包（仅已入职）；自愈不替代状态机准入。
#[register_handler_tool(
    id = "train_agent",
    name = "Train Agent (Further Learning)",
    description = "Further-train an agent so its capabilities catch up with the latest career and organization requirements. Idempotent, three phases: (1) install missing base packs (neural / skill_management / tool_management); (2) detect newly published skills under installed skill packs and reinstall those packs to fill the gaps (existing copies are refreshed too); (3) re-run binding self-heal along the agent's past lifecycle edges - career matching for non-Incubating agents and organization-required packs for onboarded ones only. Returns the tags installed or refreshed this time.",
    params = "common::api::TrainAgentRequest",
    tags = "agent_management,hr_specialist"
)]
#[generate_http_handler]
pub async fn train_agent(
    ctx: RequestContext,
    params: TrainAgentRequest,
) -> Result<TrainAgentResponse> {
    domain()
        .agent_manage()
        .train_agent(ctx, &params.agent_id)
        .await
}
