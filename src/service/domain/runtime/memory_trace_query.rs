//! Runtime Memory Trace 查询作用域校验
//!
//! 记忆轨迹位于 `agents/{agent_id}/memory/`，与 think trace 同族但**没有 project
//! 维度**（`MemoryTrace` 不带 project_id）。因此复用 [`super::think_trace_query`]
//! 的校验内核时显式关闭 project：仅有 project 作用域的请求必须 fail-closed，
//! 否则会退化成跨 Agent 全量扫描。

use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::{MAX_MEMORY_TRACE_QUERY_LIMIT, MemoryTraceQuery};
use common::error::Result;

use super::think_trace_query;

/// 合并显式查询作用域与请求上下文作用域
pub(crate) fn with_context_scope(
    ctx: RequestContext,
    mut query: MemoryTraceQuery,
) -> Result<MemoryTraceQuery> {
    let scope = think_trace_query::with_context_fields(
        ctx,
        think_trace_query::TraceScopeFields {
            agent_id: query.agent_id.clone(),
            project_id: None,
            task_id: query.task_id.clone(),
            limit: query.pagination.limit,
            max_limit: MAX_MEMORY_TRACE_QUERY_LIMIT,
            allow_project: false,
            noun: "memory trace",
        },
    )?;
    query.agent_id = scope.agent_id;
    query.task_id = scope.task_id;
    Ok(query)
}
