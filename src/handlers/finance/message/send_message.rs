//! Handler: 发送消息给用户

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToUserCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendMessageParams, SendMessageResponse};
use common::enums::CallerType;
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
    // 根据 caller_type 选择 from_agent_id 来源（替换原 agent_id fallback system 的隐式推断）
    let from_agent_id = match ctx.caller_type() {
        CallerType::Agent => ctx.agent_id().map(|s| s.to_string()).unwrap_or_default(),
        CallerType::User => ctx.uid(),
        CallerType::System => "system".to_string(),
    };

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
