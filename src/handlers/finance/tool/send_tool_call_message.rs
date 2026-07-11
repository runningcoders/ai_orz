//! Handler: 发送工具调用消息（异步，神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendToolCallRequestCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendToolCallMessageParams, SendToolCallMessageResponse};
use common::error::Result;

/// 发送工具调用消息（异步）
///
/// Agent 通过此工具发起 manual 工具的异步调用。
/// 消息发送后立即返回，工具执行结果通过 ToolCallResult 消息在下一轮 awaken 中送达。
#[register_handler_tool(
    id = "send_tool_call_message",
    name = "send_tool_call_message",
    description = "Send a tool call message (async)",
    params = "common::api::SendToolCallMessageParams",
    neural
)]
#[generate_http_handler]
pub async fn send_tool_call_message(
    ctx: RequestContext,
    params: SendToolCallMessageParams,
) -> Result<SendToolCallMessageResponse> {
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "当前请求缺少 Agent 上下文"))?
        .clone();

    let request_id = uuid::Uuid::now_v7().to_string();

    let cmd = SendToolCallRequestCommand {
        request_id: &request_id,
        tool_id: &params.tool_id,
        tool_name: &params.tool_name,
        from_agent_id: &agent_id,
        to_executor_id: "system",
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: None,
        args: params.params,
    };

    let message = message::domain()
        .delivery()
        .send_tool_call_request(ctx, cmd)
        .await?;

    Ok(SendToolCallMessageResponse {
        request_id,
        message_id: message.po.id,
        status: "dispatched".to_string(),
    })
}
