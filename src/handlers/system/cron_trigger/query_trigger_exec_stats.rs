//! Handler: GET /api/v1/system/cron-triggers/{trigger_id}/exec-stats
//!
//! 执行概览 + 时序分布，与 `/exec-records` 明细分页解耦，前端可独立拉取。
//! 时间窗缺省最近 7 天，时序桶缺省 60 分钟（限定 1~1440 分钟）。

use ai_orz_macros::generate_http_handler;
use common::api::{QueryTriggerExecStatsRequest, QueryTriggerExecStatsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::TriggerExecStatsQuery;
use crate::service::domain::system::domain;

use super::query_trigger_exec::{DEFAULT_WINDOW_MS, resolve_exec_org};

const DEFAULT_BUCKET_MINUTES: i64 = 60;
const MIN_BUCKET_MINUTES: i64 = 1;
const MAX_BUCKET_MINUTES: i64 = 1440;

#[generate_http_handler]
pub async fn query_trigger_exec_stats(
    ctx: RequestContext,
    params: QueryTriggerExecStatsRequest,
) -> Result<QueryTriggerExecStatsResponse> {
    let now = chrono::Utc::now().timestamp_millis();
    let start_at = params.start_at.unwrap_or(now - DEFAULT_WINDOW_MS);
    let end_at = params.end_at.unwrap_or(now);
    let bucket_ms = params
        .bucket_minutes
        .unwrap_or(DEFAULT_BUCKET_MINUTES)
        .clamp(MIN_BUCKET_MINUTES, MAX_BUCKET_MINUTES)
        * 60_000;

    let org_id = resolve_exec_org(&ctx, &params.trigger_id).await?;
    let result = domain()
        .cron_manager()
        .query_trigger_exec_stats(
            ctx,
            TriggerExecStatsQuery {
                trigger_id: params.trigger_id,
                org_id,
                time_range: (start_at, end_at),
                bucket_ms,
            },
        )
        .await?;

    Ok(QueryTriggerExecStatsResponse {
        summary: result.summary,
        buckets: result.buckets,
    })
}
