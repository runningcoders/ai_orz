//! Handler: POST /api/v1/messages/agents - Send message to an agent

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToAgentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendMessageToAgentParams, SendMessageToAgentResponse};
use common::error::Result;
use common::enums::MessageRole;

/// Send a message to another AI agent (for collaboration)
#[register_handler_tool(
    id = "send_message_to_agent",
    name = "send_message_to_agent",
    description = "Send a message to another AI agent for collaboration. The target agent will be awakened to process the message.",
    params = "common::api::SendMessageToAgentParams",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn send_message_to_agent(
    ctx: RequestContext,
    params: SendMessageToAgentParams,
) -> Result<SendMessageToAgentResponse> {
    let from_id = ctx
        .agent_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "system".to_string());

    let from_role = if ctx.agent_id().is_some() {
        MessageRole::Agent
    } else {
        MessageRole::System
    };

    let cmd = SendToAgentCommand {
        from_id: &from_id,
        from_role,
        to_agent_id: &params.to_agent_id,
        content: &params.content,
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: params.reply_to_id.as_deref(),
    };

    let message = message::domain().delivery().send_to_agent(ctx, cmd).await?;

    Ok(SendMessageToAgentResponse {
        message_id: message.po.id,
    })
}