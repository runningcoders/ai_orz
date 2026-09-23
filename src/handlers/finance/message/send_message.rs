//! Handler: 发送消息给用户

use super::auto_reply_to_id;
use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToUserCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendMessageParams, SendMessageResponse};
use common::error::Result;

/// 发送消息给用户
///
/// `to_user_id` 必须是**人类用户**的 ID：框架在 domain 层按「收件人角色 ⟷ ID」校验，
/// 传同伴 Agent 的 ID 会直接被拒（并提示改用 `send_message_to_agent`）。
/// 历史上模型常把 Agent ID 塞进这里，落库成 `to_role=User + to_id=<Agent>` 的死信
/// （投递全失败 → 重试 8 次 → 静默丢弃），故描述里显式写明收件人类型。
#[register_handler_tool(
    id = "send_message",
    name = "Send Chat Message",
    description = "Send a chat message from the current agent to a human user (to_user_id MUST be a real user id, never another agent's id), optionally scoped with project_id, task_id, or reply_to_id to thread the reply. Returns the message_id. To message another AI agent use send_message_to_agent; to assign work to another agent use send_task_assignment_message.",
    params = "common::api::SendMessageParams",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn send_message(
    ctx: RequestContext,
    params: SendMessageParams,
) -> Result<SendMessageResponse> {
    // 发送方 = 消息的发送者本人：后台唤醒场景 ctx 的 caller_type 是 System，
    // 但执行者是被唤醒的 Agent（项目 / 任务 owner Agent），见 message_sender_id
    let from_agent_id = ctx.message_sender_id();
    let reply_to_id = auto_reply_to_id(&ctx, params.reply_to_id.as_deref());

    let cmd = SendToUserCommand {
        from_agent_id: &from_agent_id,
        to_user_id: &params.to_user_id,
        content: &params.content,
        // 写路径归一化：默认会话哨兵折叠回 None（落库仍为 NULL），见
        // `common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID`
        project_id: common::constants::message::normalize_project_id(params.project_id.as_deref()),
        task_id: params.task_id.as_deref(),
        reply_to_id: reply_to_id.as_deref(),
    };

    let message = message::domain().delivery().send_to_user(ctx, cmd).await?;

    Ok(SendMessageResponse {
        message_id: message.po.id,
    })
}
