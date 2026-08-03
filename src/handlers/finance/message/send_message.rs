//! Handler: 发送消息给用户

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToUserCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendMessageParams, SendMessageResponse};
use common::error::Result;

/// 发送消息给用户
#[register_handler_tool(
    id = "send_message",
    name = "send_message",
    description = "Send a text message from the current agent to a target user. Supports optional project_id, task_id and reply_to_id for contextualizing the message. Use this to notify users or reply within a conversation.",
    params = "common::api::SendMessageParams",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn send_message(
    ctx: RequestContext,
    params: SendMessageParams,
) -> Result<SendMessageResponse> {
    // 调用方身份由 ctx 封装方法统一提供
    let from_agent_id = ctx.caller_id_or_system();

    let cmd = SendToUserCommand {
        from_agent_id: &from_agent_id,
        to_user_id: &params.to_user_id,
        content: &params.content,
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: params.reply_to_id.as_deref(),
    };

    let message = message::domain().delivery().send_to_user(ctx, cmd).await?;

    Ok(SendMessageResponse {
        message_id: message.po.id,
    })
}
