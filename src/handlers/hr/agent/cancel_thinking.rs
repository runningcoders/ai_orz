//! Handler: POST /api/v1/hr/agents/{id}/cancel-thinking - 取消 Agent 思考

use crate::pkg::RequestContext;
use crate::service::domain::runtime::{CancelOutcome, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CancelThinkingRequest, CancelThinkingResponse};
use common::error::Result;

/// 取消 Agent 正在进行的思考（触发 cancel_flag，Agent 在当前轮次完成后退出）
///
/// `neural` 必需（原先只挂 `collaboration`，而该 tag 从未进过任何 Agent 的
/// `installed_tags` ⇒ 工具**不在任何 Agent 的工具面**里，是死工具）。
/// 它是 `recall_message` 的**互补原语**：`recall_message` 决定「这条消息还要不要
/// 处理」（状态语义），本工具决定「现在这一轮停不停」（信号语义）——
/// 撤回别人正在处理的消息时内部就是调它；撤回「自己正在处理的消息」会被
/// `recall_message` 拒绝并引导改用本工具，若它不可达该引导就是空话。
///
/// 权限：**只做存在性校验，不设关系门**（与 `send_message_to_agent` /
/// `send_task_assignment_message` 等既有跨 Agent 工具一致：Agent 工具面不设授权层，
/// 否则会造出「能发消息、能派任务，却停不了对方」的能力错位）。判据与信号投递都在
/// Domain 层单点实现（`RuntimeDomain::cancel_thinking`），本 handler 只做 DTO 映射。
#[register_handler_tool(
    id = "cancel_thinking",
    name = "Cancel Agent Thinking",
    description = "Ask an agent (by id) to stop its ongoing thinking round: a cancel signal is delivered and the agent exits after finishing the current round. Returns success=true plus a message, or success=false if the agent exists but is not currently thinking. An unknown agent id returns an error. Use it when an agent appears stuck in a long thinking loop, or when you want to interrupt your own current round. To instead invalidate a queued message so its receiver is never awakened, use recall_message.",
    params = "common::api::CancelThinkingRequest",
    neural,
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn cancel_thinking(
    ctx: RequestContext,
    params: CancelThinkingRequest,
) -> Result<CancelThinkingResponse> {
    let outcome = domain().cancel_thinking(ctx, &params.id).await?;

    Ok(match outcome {
        CancelOutcome::Cancelled => CancelThinkingResponse {
            success: true,
            message: "已发送取消信号，Agent 将在当前轮次完成后退出思考".to_string(),
        },
        CancelOutcome::NotThinking => CancelThinkingResponse {
            success: false,
            message: "Agent 当前未在思考，无需取消".to_string(),
        },
    })
}
