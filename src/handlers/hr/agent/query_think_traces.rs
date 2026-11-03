//! Handler: GET /api/v1/hr/agents/think-traces - 分页查询轮级思考轨迹
//!
//! 两种调用姿势共用同一入口：
//! - Agent 详情页：带 `agent_id`，只看单个 Agent 的历史轨迹；
//! - 对话页右侧「思考」Tab：项目对话带 `project_id` 聚合多 Agent，
//!   默认对话带前台 `agent_id`（与工具调用列表同源的组织方式）。

use crate::pkg::RequestContext;
use crate::pkg::think_trace::{ThinkTraceEntry, ThinkTraceQuery};
use crate::service::domain::runtime::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    PagedResult, QueryThinkTracesRequest, ThinkTraceListItem, ThinkTraceOccurrenceDto,
    ThinkTraceRoundDto, ThinkTraceRouteDto, ThinkTraceToolCallDto,
};
use common::error::Result;

/// 分页查询 Agent 思考轨迹（节点序列 + 选路来源）
#[register_handler_tool(
    id = "query_think_traces",
    name = "Query Think Traces",
    description = "Page through round-level thinking traces (node occurrences and route choices) for one agent or across agents in a project. Use it to diagnose why a dynamic route took a branch; omit agent_id for a multi-agent feed.",
    params = "common::api::QueryThinkTracesRequest",
    tags = "runtime,query"
)]
#[generate_http_handler]
pub async fn query_think_traces(
    ctx: RequestContext,
    params: QueryThinkTracesRequest,
) -> Result<PagedResult<ThinkTraceListItem>> {
    let page = domain()
        .query_think_traces(
            ctx,
            ThinkTraceQuery {
                agent_id: params.agent_id,
                project_id: params.project_id,
                task_id: params.task_id,
                run_id: params.run_id,
                scene: params.scene,
                started_after: params.started_after,
                started_before: params.started_before,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(to_list_item))
}

/// 内部轨迹条目 → 对外 DTO
fn to_list_item(entry: ThinkTraceEntry) -> ThinkTraceListItem {
    ThinkTraceListItem {
        run_id: entry.run_id,
        agent_id: entry.agent_id,
        scene: entry.scene,
        path: entry.path,
        steps: entry.steps,
        advisor_calls: entry.advisor_calls,
        fingerprint: entry.fingerprint,
        created_at: entry.created_at,
        // 对外的「是否结束」= 内部落盘粒度 `partial` 的反义：
        // 收尾行到达后 `merge_into` 会把 partial 置 false，此处天然跟随
        finished: !entry.partial,
        project_id: entry.project_id,
        task_id: entry.task_id,
        rounds: entry
            .rounds
            .into_iter()
            .map(|r| ThinkTraceRoundDto {
                round: r.round,
                assistant_text: r.assistant_text,
                reasoning: r.reasoning,
                tool_calls: r
                    .tool_calls
                    .into_iter()
                    .map(|t| ThinkTraceToolCallDto {
                        call_id: t.call_id,
                        name: t.name,
                        arguments: t.arguments,
                        result: t.result,
                        status: t.status,
                    })
                    .collect(),
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                cached_tokens: r.cached_tokens,
                reasoning_tokens: r.reasoning_tokens,
                duration_ms: r.duration_ms,
                decision: r.decision,
                thinking_intensity: r.thinking_intensity,
            })
            .collect(),
        occurrences: entry
            .occurrences
            .into_iter()
            .map(|o| ThinkTraceOccurrenceDto {
                node: o.node,
                seq: o.seq,
                step: o.step,
                signal: o.signal,
            })
            .collect(),
        routes: entry
            .routes
            .into_iter()
            .map(|r| ThinkTraceRouteDto {
                step: r.step,
                candidates: r.candidates,
                chosen: r.chosen,
                via: r.via,
                reason: r.reason,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_mirrors_partial_so_frontend_can_tell_running_aps() {
        // 落盘契约用 `partial` 表达「增量行 vs 收尾行」，对外统一成 `finished`。
        // 这一层必须严格反义：前端据此显示「进行中」，反了就会把跑完的标成在跑。
        let running = to_list_item(ThinkTraceEntry {
            partial: true,
            ..Default::default()
        });
        assert!(!running.finished, "只有增量行的 run 仍在进行中");

        let done = to_list_item(ThinkTraceEntry {
            partial: false,
            ..Default::default()
        });
        assert!(done.finished, "收尾行到达后即为已结束");
    }
}
