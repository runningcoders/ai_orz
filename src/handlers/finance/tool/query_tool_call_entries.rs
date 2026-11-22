//! Handler: GET /api/v1/tool-call-entries - Query tool call trace entries
//!
//! 对外出口：返回前用 [`redact!`] 对 entries 做脱敏（内部存储保持原文）。

use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallQuery;
use crate::service::domain::runtime;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{QueryToolCallEntriesRequest, QueryToolCallEntriesResponse};
use common::error::Result;

/// Query tool call trace entries with common filters.
#[register_handler_tool(
    id = "query_tool_call_entries",
    name = "Query Tool Call Records",
    description = "Filter tool call trace records by structured fields (call_id, tool_id, agent_id, project_id, task_id, status, started_after/started_before) with limit/offset pagination. Returns { items, total } where items carry arguments, results, and timing; use get_tool_call_entry when you already know the call_id.",
    params = "common::api::QueryToolCallEntriesRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn query_tool_call_entries(
    ctx: RequestContext,
    params: QueryToolCallEntriesRequest,
) -> Result<QueryToolCallEntriesResponse> {
    let page = runtime::domain()
        .tool_execution()
        .query_tool_call_entries(
            ctx,
            ToolCallQuery {
                call_id: params.call_id,
                agent_id: params.agent_id,
                agent_ids: common::api::split_csv_ids(params.agent_ids.as_deref()),
                project_id: params.project_id,
                without_project: params.without_project.unwrap_or(false),
                task_id: params.task_id,
                tool_id: params.tool_id,
                status: params
                    .status
                    .map(crate::service::domain::runtime::status_from_dto),
                started_after: params.started_after,
                started_before: params.started_before,
                limit: params.limit,
                offset: params.offset.unwrap_or(0),
            },
        )
        .await?;

    let response: QueryToolCallEntriesResponse = page.map(Into::into);
    Ok(crate::redact!(response)?)
}
