//! AOP 实时统计 HTTP 接口（3 个端点）
//!
//! 直接读取内存中的 AopStatsCollector 快照，零 DB 查询，毫秒级响应。
//!
//! 读口在 handler：collector 是纯基础设施单例（`pkg::stats::runtime::aop`），
//! 经 SystemDomain 转一手只会让 `domain → consumer` 成立，没有任何领域逻辑可放。

use ai_orz_macros::generate_http_handler;
use common::api::{
    AopStatsDistributionItem, AopStatsDistributionResponse, AopStatsOverviewResponse,
    AopStatsTimeSeriesPoint, AopStatsTimeSeriesResponse, GetStatsDistributionRequest,
    GetStatsOverviewRequest, GetStatsTimeSeriesRequest,
};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::pkg::stats::runtime::aop;

/// 取全局收集器；未初始化时按内部错误返回（服务启动装配缺失）
fn collector() -> Result<&'static aop::AopStatsCollector> {
    aop::collector()
        .ok_or_else(|| common::error::Error::internal("AopStatsCollector not initialized"))
}

/// GET /api/v1/system/aop/stats/overview
#[generate_http_handler]
pub async fn get_stats_overview(
    _ctx: RequestContext,
    _params: GetStatsOverviewRequest,
) -> Result<AopStatsOverviewResponse> {
    let result = collector()?.overview().await;
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
    _ctx: RequestContext,
    params: GetStatsTimeSeriesRequest,
) -> Result<AopStatsTimeSeriesResponse> {
    // 请求侧是严格枚举（§3.6）；统计链路按线格式字符串过滤，转换收口在此
    let event_kind = params.event_kind.map(|t| t.as_str().to_string());
    let points = collector()?
        .time_series(
            event_kind.as_deref(),
            params.consumer_name.as_deref(),
            params.status.as_deref(),
        )
        .await;

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
    _ctx: RequestContext,
    params: GetStatsDistributionRequest,
) -> Result<AopStatsDistributionResponse> {
    let items = collector()?
        .distribution(&params.group_by, params.status.as_deref())
        .await;

    let items: Vec<AopStatsDistributionItem> = items
        .into_iter()
        .map(|i| AopStatsDistributionItem {
            label: i.label,
            value: i.value,
        })
        .collect();
    Ok(AopStatsDistributionResponse { items })
}
