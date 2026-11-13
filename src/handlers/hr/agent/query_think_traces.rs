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
    PagedResult, QueryThinkTracesRequest, ThinkTraceCerebellumDto, ThinkTraceListItem,
    ThinkTraceOccurrenceDto, ThinkTraceRoundDto, ThinkTraceRouteDto, ThinkTraceToolCallDto,
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
                agent_ids: common::api::split_csv_ids(params.agent_ids.as_deref()),
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
        // run 级小脑留痕：收尾行的全量 + chain_reply 补丁行补的那批（查询侧
        // merge_into 已按 (agent_id, run_id, scene) 拼好，这里只做结构映射）
        cerebellum: entry
            .cerebellum
            .into_iter()
            .map(|c| ThinkTraceCerebellumDto {
                scene: c.scene,
                decision: c.decision,
                confidence: c.confidence,
                degraded: c.degraded,
                applied: c.applied,
                latency_ms: c.latency_ms,
            })
            .collect(),
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

    #[test]
    fn cerebellum_fields_map_one_to_one_into_dto() {
        // 「前端三维平表字段与后端契约一一对应」的落点断言。
        //
        // 手写映射（不是 From）的风险：后端 `ThinkTraceCerebellum` 将来加字段时，
        // 编译器不会提醒这里漏映射 —— 轨迹里有了、接口里没有，前端永远看不到。
        // 所以先把 6 个字段全部填成**非默认值**，再逐项比对：任何一项漏映射
        // （或被静默改成默认值）这条立即红。
        use crate::pkg::think_trace::{ThinkTraceCerebellum, ThinkTraceEntry};
        let entry = ThinkTraceEntry {
            cerebellum: vec![ThinkTraceCerebellum {
                scene: "chain_reply".to_string(),
                decision: "close".to_string(),
                confidence: Some(0.87),
                degraded: Some("circuit_open".to_string()),
                applied: true,
                latency_ms: Some(1_234),
            }],
            ..Default::default()
        };
        let dto = to_list_item(entry);
        assert_eq!(dto.cerebellum.len(), 1, "留痕条数必须原样透出");
        let c = &dto.cerebellum[0];
        assert_eq!(c.scene, "chain_reply");
        assert_eq!(c.decision, "close");
        assert_eq!(c.confidence, Some(0.87));
        assert_eq!(c.degraded.as_deref(), Some("circuit_open"));
        assert!(c.applied);
        assert_eq!(c.latency_ms, Some(1_234), "p95 时延的回收点不能丢");
    }

    #[test]
    fn empty_cerebellum_stays_empty_not_null() {
        // 空留痕必须是空数组（前端按 `is_empty()` 决定是否渲染折叠条），
        // 不能因为映射逻辑变成「一条全默认值的假记录」。
        let dto = to_list_item(ThinkTraceEntry::default());
        assert!(dto.cerebellum.is_empty());
    }
}
