//! Handler: POST /api/v1/messages/recall - 撤回消息（REST + 神经工具）
//!
//! 双宏标注：`#[register_handler_tool]`（Agent 工具）+ `#[generate_http_handler]`
//! （REST handler），**同一个函数两个出口**，不写两套实现。
//!
//! `neural` 必需：本工具与 `send_message` / `send_message_to_agent` /
//! `search_messages` 同属一条沟通链路，而 `messaging` tag 从未进过任何 Agent 的
//! `installed_tags` ⇒ 只挂 `messaging` 会让它**不在任何 Agent 的工具面**里。
//! 症状是**静默失效**而非报错（模型只能退回去重复发消息），护栏见
//! `pkg/tool_registry/builtin.rs::message_tools_are_neural_reachable`。

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, RecallOutcome};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RecallMessageRequest, RecallMessageResponse};
use common::error::Result;

/// 撤回（作废）一条「已发出但尚未被处理」的消息，使接收方不再被它唤醒。
///
/// 描述里的三条边界必须与 `RecallOutcome` 的语义一一对应，否则模型会误读/误承诺：
/// 1. 只对未处理 / 正在处理的消息有效；已处理、已失败的撤回是 no-op（不报错）；
/// 2. 撤回是「阻止后续消费」，不是时间倒流：接收方已经读到的影响无法消除，且不可撤销；
/// 3. 正在处理的消息是**尽力取消**（轮次边界生效），不保证立即停止当前 LLM 调用。
#[register_handler_tool(
    id = "recall_message",
    name = "Recall Message",
    description = "Recall (retract) a message that has been sent but not yet processed, so the receiving agent will not be awakened by it and the stale request no longer blocks the queue. Only works while the message is still pending or currently being processed: if the receiving agent is already working on it, a best-effort cancel signal is sent and it stops at the next round boundary (not an immediate interruption). Messages already processed, failed, or recalled are a no-op and are reported as such in the `outcome` field. Recall prevents FUTURE processing only - the recipient cannot un-see what it already read, and the action cannot be undone. Allowed for: the message sender, the owning user of the project, the project's owner agent, or a super admin. Use this to cancel a stale/incorrect queued instruction before it is handled; use cancel_thinking if you only need to stop an agent's current thinking round.",
    params = "common::api::RecallMessageRequest",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn recall_message(
    ctx: RequestContext,
    params: RecallMessageRequest,
) -> Result<RecallMessageResponse> {
    let outcome = message::domain()
        .recall_message(ctx, &params.message_id, params.reason.as_deref())
        .await?;

    // `outcome` 字段承载幂等语义：撤回「已处理 / 已撤回」的消息**不算失败**，
    // 因此这些分支的 `success = false` 只表示「本次未产生实际效果」，请求本身成功。
    Ok(match outcome {
        RecallOutcome::Recalled => RecallMessageResponse {
            success: true,
            outcome: "recalled".to_string(),
            message: "消息已撤回：接收方不会再被它唤醒。注意撤回不可撤销，且不改变已读上下文。"
                .to_string(),
            cancelled_agent_id: None,
        },
        RecallOutcome::RecalledInFlight {
            agent_id,
            cancelled,
        } => RecallMessageResponse {
            success: true,
            outcome: "recalled".to_string(),
            message: if cancelled {
                format!(
                    "该消息正在被 Agent {agent_id} 处理：已向它发出取消信号（将在当前轮次完成后停止），消息同时已标记为撤回"
                )
            } else {
                format!(
                    "该消息正在被 Agent {agent_id} 处理：Agent 的思考恰好在此刻结束，取消信号未送达；消息已标记为撤回，不会再被处理"
                )
            },
            cancelled_agent_id: Some(agent_id),
        },
        RecallOutcome::AlreadyRecalled => RecallMessageResponse {
            success: false,
            outcome: "already_recalled".to_string(),
            message: "该消息此前已被撤回，无需重复操作".to_string(),
            cancelled_agent_id: None,
        },
        RecallOutcome::NotRecallable { status } => RecallMessageResponse {
            success: false,
            outcome: "not_recallable".to_string(),
            message: format!(
                "该消息当前状态为 {status:?}（已处理 / 已失败），撤回没有意义；这不是错误，无需重试"
            ),
            cancelled_agent_id: None,
        },
    })
}
