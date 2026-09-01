//! Handler: GET /api/v1/tool-call-entries - Query tool call trace entries

use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallQuery;
use crate::service::domain::runtime;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{QueryToolCallEntriesRequest, QueryToolCallEntriesResponse};
use common::error::Result;

use super::response::to_tool_call_entry_detail;

/// Query tool call trace entries with common filters.
#[register_handler_tool(
    id = "query_tool_call_entries",
    name = "Query Tool Call Records",
    description = "Query tool call trace entries by call_id, agent_id, project_id, task_id, tool_id, status, and time range",
    params = "common::api::QueryToolCallEntriesRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn query_tool_call_entries(
    ctx: RequestContext,
    params: QueryToolCallEntriesRequest,
) -> Result<QueryToolCallEntriesResponse> {
    let entries = runtime::domain()
        .tool_execution()
        .query_tool_call_entries(
            ctx,
            ToolCallQuery {
                call_id: params.call_id,
                agent_id: params.agent_id,
                project_id: params.project_id,
                task_id: params.task_id,
                tool_id: params.tool_id,
                status: params
                    .status
                    .map(crate::service::domain::runtime::status_from_dto),
                started_after: params.started_after,
                started_before: params.started_before,
                limit: params.limit,
            },
        )
        .await?;

    Ok(entries.iter().map(to_tool_call_entry_detail).collect())
}
