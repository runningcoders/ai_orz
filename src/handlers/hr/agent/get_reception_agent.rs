//! Handler: GET /api/v1/hr/agents/reception - 获取当前可用的前台 Agent
//!
//! 通过 `HrDomain::resolve_agent(ctx)` 统一路由到前台 Agent：
//! - 优先 feishu_reception 角色 Onboarded Agent
//! - fallback 任意 Onboarded Agent
//!
//! **agent 与 project 是两个维度**：resolve_agent 只接受 ctx，不感知 project。

use common::api::{GetReceptionAgentRequest, GetReceptionAgentResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};

/// 获取当前可用的前台 Agent
#[register_handler_tool(
    id = "get_reception_agent",
    name = "get_reception_agent",
    description = "Resolve the current available reception agent (unified routing). Prefer feishu_reception role onboarded agent, fallback to any onboarded agent.",
    params = "common::api::GetReceptionAgentRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn get_reception_agent(
    ctx: RequestContext,
    _params: GetReceptionAgentRequest,
) -> Result<GetReceptionAgentResponse> {
    let agent = domain()
        .resolve_agent(ctx)
        .await?
        .ok_or_else(|| common::error::Error::not_found("无可用前台 Agent"))?;

    Ok(GetReceptionAgentResponse {
        agent_id: agent.po.id,
        agent_name: agent.po.name,
    })
}
