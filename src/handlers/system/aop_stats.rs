//! AOP 实时统计 HTTP 接口（3 个端点）
//!
//! 直接读取内存中的 AopStatsCollector 快照，零 DB 查询，毫秒级响应。

use ai_orz_macros::generate_http_handler;
use common::api::{
    AopStatsDistributionItem, AopStatsDistributionResponse, AopStatsOverviewResponse,
    AopStatsTimeSeriesPoint, AopStatsTimeSeriesResponse, GetStatsDistributionRequest,
    GetStatsOverviewRequest, GetStatsTimeSeriesRequest,
};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

/// GET /api/v1/system/aop/stats/overview
#[generate_http_handler]
pub async fn get_stats_overview(
    ctx: RequestContext,
    _params: GetStatsOverviewRequest,
) -> Result<AopStatsOverviewResponse> {
    let result = domain().aop_stats().overview(ctx).await?;
    Ok(AopStatsOverviewResponse {
        total_published: result.total_published,
        total_consumed: result.total_consumed,
        total_success: result.total_success,
        total_failed: result.total_failed,
        avg_duration_ms: result.avg_duration_ms,
    })
}

/// GET /api/v1/system/aop/stats/time-series
#[generate_http_handler]
pub async fn get_stats_time_series(
    ctx: RequestContext,
    params: GetStatsTimeSeriesRequest,
) -> Result<AopStatsTimeSeriesResponse> {
    let points = domain()
        .aop_stats()
        .time_series(ctx, params.event_kind, params.consumer_name, params.status)
        .await?;

    let points: Vec<AopStatsTimeSeriesPoint> = points
        .into_iter()
        .map(|p| AopStatsTimeSeriesPoint {
            interval_start: p.interval_start,
            call_count: p.call_count,
        })
        .collect();
    Ok(AopStatsTimeSeriesResponse { points })
}

/// GET /api/v1/system/aop/stats/distribution
#[generate_http_handler]
pub async fn get_stats_distribution(
    ctx: RequestContext,
    params: GetStatsDistributionRequest,
) -> Result<AopStatsDistributionResponse> {
    let items = domain()
        .aop_stats()
        .distribution(ctx, params.group_by, params.status)
        .await?;

    let items: Vec<AopStatsDistributionItem> = items
        .into_iter()
        .map(|i| AopStatsDistributionItem {
            label: i.label,
            value: i.value,
        })
        .collect();
    Ok(AopStatsDistributionResponse { items })
}
