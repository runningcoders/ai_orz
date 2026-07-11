//! Handler: 请求工具调用（同步，HTTP API 专用）

use crate::pkg::RequestContext;
use crate::service::domain::runtime;
use ai_orz_macros::generate_http_handler;
use common::api::{RequestToolCallParams, RequestToolCallResponse};
use common::error::Result;

/// 请求工具调用（同步）
///
/// 注意：此 Handler 不注册为 Agent 工具。
/// Agent 异步调用工具应使用 `send_tool_call_message` 神经工具。
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
