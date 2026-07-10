//! Handler: 请求工具调用（异步）

use crate::pkg::RequestContext;
use crate::service::domain::runtime;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RequestToolCallParams, RequestToolCallResponse};
use common::error::Result;

/// 请求工具调用
#[register_handler_tool(
    id = "request_tool_call",
    name = "request_tool_call",
    description = "Request a manual tool call for the agent",
    params = "common::api::RequestToolCallParams"
)]
#[generate_http_handler]
pub async fn request_tool_call(
    ctx: RequestContext,
    params: RequestToolCallParams,
) -> Result<RequestToolCallResponse> {
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| common::error::err!(InvalidRequest, "当前请求缺少 Agent 上下文"))?
        .clone();

    let result = runtime::domain()
        .tool_execution()
        .call_manual_tool_for_agent(ctx, agent_id, params.tool_id, params.params)
        .await?;

    Ok(RequestToolCallResponse {
        tool_call_id: result.trace_ref.call_id,
        status: "completed".to_string(),
    })
}
