//! Handler: 记忆轨迹查询
//!
//! - `GET /api/v1/hr/agents/memory-traces`：分页查询（列表只带预览）
//! - `GET /api/v1/hr/agents/memory-traces/{trace_id}`：详情（完整 input/output）
//!
//! 两个入口共用 Domain 层作用域校验：Agent 详情页带 `agent_id`，对话侧
//! 凭 think trace 的 `run_id`（即 MemoryTrace.id）+ `agent_id` 拉取详情。

use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryTraceQuery;
use crate::service::domain::runtime::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    GetMemoryTraceRequest, MemoryTraceDetail, MemoryTraceListItem, PagedResult,
    QueryMemoryTracesRequest,
};
use common::error::Result;

/// 预览截断长度（按字符计，避免截断到 UTF-8 字节中间）
const PREVIEW_MAX_CHARS: usize = 200;

/// 分页查询记忆轨迹
#[register_handler_tool(
    id = "query_memory_traces",
    name = "Query Memory Traces",
    description = "Page through raw memory traces (input/output of each thinking cycle) for one agent or across agents. Use it to inspect what prompt an agent received and what it produced in a specific run; pair trace_id with think traces by run_id.",
    params = "common::api::QueryMemoryTracesRequest",
    tags = "runtime,query"
)]
#[generate_http_handler]
pub async fn query_memory_traces(
    ctx: RequestContext,
    params: QueryMemoryTracesRequest,
) -> Result<PagedResult<MemoryTraceListItem>> {
    let page = domain()
        .query_memory_traces(
            ctx,
            MemoryTraceQuery {
                agent_id: params.agent_id,
                task_id: params.task_id,
                log_id: params.log_id,
                trace_id: params.trace_id,
                created_after: params.created_after,
                created_before: params.created_before,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(to_list_item))
}

/// 读取单条记忆轨迹详情（完整输入输出）
#[generate_http_handler]
pub async fn get_memory_trace(
    ctx: RequestContext,
    params: GetMemoryTraceRequest,
) -> Result<MemoryTraceDetail> {
    let trace = domain()
        .get_memory_trace(ctx, &params.agent_id, &params.trace_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("memory trace {} not found", params.trace_id))
        })?;

    Ok(MemoryTraceDetail {
        id: trace.id,
        agent_id: trace.agent_id,
        task_id: trace.task_id,
        log_id: trace.log_id,
        user_id: trace.user_id,
        organization_id: trace.organization_id,
        role: trace.role.to_string(),
        created_at: trace.created_at,
        completed_at: trace.completed_at,
        input: trace.input,
        output: trace.output,
        metadata: trace.metadata,
    })
}

/// 内部轨迹 → 列表 DTO（input/output 截断为预览）
fn to_list_item(trace: crate::models::memory::MemoryTrace) -> MemoryTraceListItem {
    MemoryTraceListItem {
        id: trace.id,
        agent_id: trace.agent_id,
        task_id: trace.task_id,
        log_id: trace.log_id,
        user_id: trace.user_id,
        organization_id: trace.organization_id,
        role: trace.role.to_string(),
        created_at: trace.created_at,
        completed_at: trace.completed_at,
        input_preview: preview(&trace.input),
        output_preview: trace.output.as_deref().map(preview),
        metadata: trace.metadata,
    }
}

/// 截断为预览（按字符边界）
fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(PREVIEW_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}
