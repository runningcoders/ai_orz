//! Handler: 请求工具调用（同步，神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::runtime;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RequestToolCallParams, RequestToolCallResponse};
use common::error::Result;

/// 请求工具调用（同步）
///
/// Agent 通过此神经工具同步调用 manual 工具，在同一轮 awaken 内拿到执行结果。
/// 与 `send_tool_call_message`（异步）对应，Agent 按需选择：
/// - 同步：本工具，结果立即可用，适合轻量、快速的工具
/// - 异步：send_tool_call_message，结果在下一轮 awaken 送达，适合耗时较长的工具
#[register_handler_tool(
    id = "request_tool_call",
    name = "request_tool_call",
    description = "Call a manual tool synchronously and get the result immediately",
    params = "common::api::RequestToolCallParams",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn request_tool_call(
    ctx: RequestContext,
    params: RequestToolCallParams,
) -> Result<RequestToolCallResponse> {
    let mut builder = ctx.to_builder();
    if let Some(project_id) = &params.project_id {
        builder = builder.project_id(project_id.clone());
    }
    if let Some(task_id) = &params.task_id {
        builder = builder.task_id(task_id.clone());
    }
    let ctx = builder.build();

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
        result: result.result,
    })
}
