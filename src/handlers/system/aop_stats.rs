//! AOP 实时统计 HTTP 接口（3 个端点）
//!
//! 直接读取内存中的 AopStatsCollector 快照，零 DB 查询，毫秒级响应。
//! 遵循现有 aop.rs 模式：直接返回 Json<ApiResponse<T>>，Response struct 定义在本文件内。

use axum::{
    Json,
    extract::Query,
};
use common::api::ApiResponse;
use serde::Deserialize;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

/// AOP 实时统计概览响应
#[derive(Debug, serde::Serialize)]
pub struct AopStatsOverviewResponse {
    pub total_published: u64,
    pub total_consumed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub avg_duration_ms: f64,
}

/// AOP 实时统计时序数据点
#[derive(Debug, serde::Serialize)]
pub struct AopStatsTimeSeriesPoint {
    pub interval_start: i64,
    pub call_count: u64,
}

/// AOP 实时统计时序响应
#[derive(Debug, serde::Serialize)]
pub struct AopStatsTimeSeriesResponse {
    pub points: Vec<AopStatsTimeSeriesPoint>,
}

/// AOP 实时统计分布项
#[derive(Debug, serde::Serialize)]
pub struct AopStatsDistributionItem {
    pub label: String,
    pub value: u64,
}

/// AOP 实时统计分布响应
#[derive(Debug, serde::Serialize)]
pub struct AopStatsDistributionResponse {
    pub items: Vec<AopStatsDistributionItem>,
}

/// 时序查询参数
#[derive(Debug, Deserialize, Default)]
pub struct TimeSeriesQuery {
    pub event_kind: Option<String>,
    pub consumer_name: Option<String>,
    pub status: Option<String>,
}

/// 分布查询参数
#[derive(Debug, Deserialize)]
pub struct DistributionQuery {
    pub group_by: String, // "consumer" | "status" | "kind"
    pub status: Option<String>,
}

/// GET /api/v1/system/aop/stats/overview
pub async fn get_stats_overview() -> Json<ApiResponse<AopStatsOverviewResponse>> {
    let ctx = RequestContext::new(None, None);
    match domain().aop_stats().overview(ctx).await {
        Ok(result) => Json(ApiResponse::success(AopStatsOverviewResponse {
            total_published: result.total_published,
            total_consumed: result.total_consumed,
            total_success: result.total_success,
            total_failed: result.total_failed,
            avg_duration_ms: result.avg_duration_ms,
        })),
        Err(e) => Json(ApiResponse {
            code: 500,
            message: format!("AOP stats overview failed: {:?}", e),
            data: None,
        }),
    }
}

/// GET /api/v1/system/aop/stats/time-series
pub async fn get_stats_time_series(
    Query(params): Query<TimeSeriesQuery>,
) -> Json<ApiResponse<AopStatsTimeSeriesResponse>> {
    let ctx = RequestContext::new(None, None);
    match domain()
        .aop_stats()
        .time_series(ctx, params.event_kind, params.consumer_name, params.status)
        .await
    {
        Ok(points) => {
            let points: Vec<AopStatsTimeSeriesPoint> = points
                .into_iter()
                .map(|p| AopStatsTimeSeriesPoint {
                    interval_start: p.interval_start,
                    call_count: p.call_count,
                })
                .collect();
            Json(ApiResponse::success(AopStatsTimeSeriesResponse { points }))
        }
        Err(e) => Json(ApiResponse {
            code: 500,
            message: format!("AOP stats time-series failed: {:?}", e),
            data: None,
        }),
    }
}

/// GET /api/v1/system/aop/stats/distribution
pub async fn get_stats_distribution(
    Query(params): Query<DistributionQuery>,
) -> Json<ApiResponse<AopStatsDistributionResponse>> {
    let ctx = RequestContext::new(None, None);
    match domain()
        .aop_stats()
        .distribution(ctx, params.group_by, params.status)
        .await
    {
        Ok(items) => {
            let items: Vec<AopStatsDistributionItem> = items
                .into_iter()
                .map(|i| AopStatsDistributionItem {
                    label: i.label,
                    value: i.value,
                })
                .collect();
            Json(ApiResponse::success(AopStatsDistributionResponse { items }))
        }
        Err(e) => Json(ApiResponse {
            code: 500,
            message: format!("AOP stats distribution failed: {:?}", e),
            data: None,
        }),
    }
}
