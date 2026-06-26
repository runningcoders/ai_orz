//! Runtime Tool Call 查询辅助逻辑

use common::error::{Error, Result};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallStatus;
use crate::pkg::tool_tracing::logger::{MAX_TOOL_CALL_QUERY_LIMIT, ToolCallQuery};

/// Merge explicit query scope with RequestContext scope and fail closed when no scope exists.
pub(crate) fn with_context_scope(
    ctx: RequestContext,
    mut query: ToolCallQuery,
) -> Result<ToolCallQuery> {
    if ctx.agent_id.is_none() && ctx.project_id.is_none() && ctx.task_id.is_none() {
        return Err(common::error::Error::bad_request(
            "tool call query requires scoped request context".to_string(),
        ));
    }

    ensure_scope_does_not_conflict(
        "agent_id",
        ctx.agent_id.as_deref(),
        query.agent_id.as_deref(),
    )?;
    ensure_scope_does_not_conflict(
        "project_id",
        ctx.project_id.as_deref(),
        query.project_id.as_deref(),
    )?;
    ensure_scope_does_not_conflict("task_id", ctx.task_id.as_deref(), query.task_id.as_deref())?;

    if let Some(limit) = query.limit {
        if limit > MAX_TOOL_CALL_QUERY_LIMIT {
            return Err(common::error::Error::bad_request(format!(
                "tool call query limit must be <= {MAX_TOOL_CALL_QUERY_LIMIT}"
            )));
        }
    }

    if query.agent_id.is_none() {
        query.agent_id = ctx.agent_id.clone();
    }
    if query.project_id.is_none() {
        query.project_id = ctx.project_id.clone();
    }
    if query.task_id.is_none() {
        query.task_id = ctx.task_id.clone();
    }

    Ok(query)
}

pub(crate) fn ensure_call_id_present(query: &ToolCallQuery) -> Result<()> {
    match query.call_id.as_deref() {
        Some(call_id) if !call_id.trim().is_empty() => Ok(()),
        _ => Err(common::error::Error::bad_request(
            "tool call detail lookup requires call_id".to_string(),
        )),
    }
}

fn ensure_scope_does_not_conflict(
    field: &str,
    context_value: Option<&str>,
    query_value: Option<&str>,
) -> Result<()> {
    match (context_value, query_value) {
        (Some(context_value), Some(query_value)) if context_value != query_value => {
            Err(common::error::Error::bad_request(format!(
                "tool call query {field} conflicts with request context"
            )))
        }
        (None, Some(_)) => Err(common::error::Error::bad_request(format!(
            "tool call query {field} requires matching request context scope"
        ))),
        _ => Ok(()),
    }
}

pub(crate) fn status_from_dto(status: common::api::ToolCallStatusDto) -> ToolCallStatus {
    match status {
        common::api::ToolCallStatusDto::Started => ToolCallStatus::Started,
        common::api::ToolCallStatusDto::Completed => ToolCallStatus::Completed,
        common::api::ToolCallStatusDto::Failed => ToolCallStatus::Failed,
    }
}
