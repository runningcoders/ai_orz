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
    description = "Send a message to a user",
    params = "common::api::SendMessageParams",
    neural
)]
#[generate_http_handler]
pub async fn send_message(
    ctx: RequestContext,
    params: SendMessageParams,
) -> Result<SendMessageResponse> {
    let from_agent_id = ctx
        .agent_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "system".to_string());

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
