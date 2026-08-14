//! Handler: POST /api/v1/hr/agents/{id}/cancel-thinking - 取消 Agent 思考

use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CancelThinkingRequest, CancelThinkingResponse};
use common::error::Result;

/// 取消 Agent 正在进行的思考（触发 cancel_flag，Agent 在当前轮次完成后退出）
#[register_handler_tool(
    id = "cancel_thinking",
    name = "cancel_thinking",
    description = "Cancel an agent's ongoing thinking process. The agent will exit after completing the current round. Use this when an agent is stuck in a long thinking loop.",
    params = "common::api::CancelThinkingRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn cancel_thinking(
    ctx: RequestContext,
    params: CancelThinkingRequest,
) -> Result<CancelThinkingResponse> {
    let runtime = domain();
    let success = runtime.cancel_thinking(&params.id);

    let message = if success {
        "已发送取消信号，Agent 将在当前轮次完成后退出思考".to_string()
    } else {
        "Agent 当前未在思考，无需取消".to_string()
    };

    log_info!(
        &ctx,
        "cancel_thinking",
        "agent_id={}, success={}",
        params.id,
        success
    );

    Ok(CancelThinkingResponse { success, message })
}
