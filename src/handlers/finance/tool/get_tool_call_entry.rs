//! Handler: GET /api/v1/tool-call-entries/{call_id} - Get one tool call trace entry
//!
//! 对外出口：返回前用 [`redact!`] 对 entry 做脱敏（内部存储保持原文）。

use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallQuery;
use crate::service::domain::runtime;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetToolCallEntryRequest, GetToolCallEntryResponse};
use common::error::Result;

/// Get one tool call trace entry by call ID.
#[register_handler_tool(
    id = "get_tool_call_entry",
    name = "Get Tool Call Record",
    description = "Fetch a single tool call trace record by its call_id, including arguments, result, status, and timing. Fails with not found if no matching call exists.",
    params = "common::api::GetToolCallEntryRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn get_tool_call_entry(
    ctx: RequestContext,
    params: GetToolCallEntryRequest,
) -> Result<GetToolCallEntryResponse> {
    let entry = runtime::domain()
        .tool_execution()
        .get_tool_call_entry_by_id(
            ctx,
            ToolCallQuery {
                call_id: Some(params.call_id.clone()),
                tool_id: params.tool_id,
                agent_id: params.agent_id,
                project_id: params.project_id,
                task_id: params.task_id,
                limit: Some(1),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Tool call {} not found", params.call_id))
        })?;

    let response: GetToolCallEntryResponse = entry.into();
    Ok(crate::redact!(response)?)
}
