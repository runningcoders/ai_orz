//! Handler: GET /api/v1/system/logs/stats/level-distribution - 日志级别分布
//! Handler: GET /api/v1/system/logs/stats/time-series - 日志时序
//!
//! 遵循 aop_stats.rs 模式：系统级 RequestContext（无用户身份）+ 直接返回 Json<ApiResponse<T>>。
//! 路由层 require_role_middleware(UserRole::Admin) 已确保 Admin/SuperAdmin 可访问。

use axum::{
    Json,
    extract::Query,
};
use common::api::{
    ApiResponse, LogLevelDistributionItem, LogLevelDistributionResponse, LogStatsQueryParams,
    LogTimeSeriesPoint, LogTimeSeriesResponse,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

/// 当前 unix 毫秒时间戳
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// GET /api/v1/system/logs/stats/level-distribution
///
/// 返回日志级别分布（INFO/WARN/ERROR/DEBUG/TRACE 各自计数）。
/// 时间范围由 query 参数控制，默认最近 24 小时。
pub async fn get_log_level_distribution(
    Query(params): Query<LogStatsQueryParams>,
) -> Json<ApiResponse<LogLevelDistributionResponse>> {
    let now = now_ms();
    let end_time = params.end_time.unwrap_or(now);
    let start_time = params.start_time.unwrap_or(end_time - 24 * 60 * 60 * 1000);

    let ctx = RequestContext::new(None, None);
    match domain()
        .log_query()
        .level_distribution(ctx, start_time, end_time)
        .await
    {
        Ok(distribution) => {
            let items: Vec<LogLevelDistributionItem> = distribution
                .into_iter()
                .map(|(level, count)| LogLevelDistributionItem { level, count })
                .collect();
            let total: u64 = items.iter().map(|i| i.count).sum();
            Json(ApiResponse::success(LogLevelDistributionResponse { items, total }))
        }
        Err(e) => Json(ApiResponse {
            code: 500,
            message: format!("查询日志级别分布失败: {:?}", e),
            data: None,
        }),
    }
}

/// GET /api/v1/system/logs/stats/time-series
///
/// 返回日志时序数据（按小时桶）。
pub async fn get_log_time_series(
    Query(params): Query<LogStatsQueryParams>,
) -> Json<ApiResponse<LogTimeSeriesResponse>> {
    let now = now_ms();
    let end_time = params.end_time.unwrap_or(now);
    let start_time = params.start_time.unwrap_or(end_time - 24 * 60 * 60 * 1000);

    let ctx = RequestContext::new(None, None);
    match domain()
        .log_query()
        .time_series(ctx, start_time, end_time)
        .await
    {
        Ok(points) => {
            let points: Vec<LogTimeSeriesPoint> = points
                .into_iter()
                .map(|(interval_start, count)| LogTimeSeriesPoint {
                    interval_start,
                    count,
                })
                .collect();
            Json(ApiResponse::success(LogTimeSeriesResponse { points }))
        }
        Err(e) => Json(ApiResponse {
            code: 500,
            message: format!("查询日志时序失败: {:?}", e),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_stats_query_params_default() {
        let params = LogStatsQueryParams {
            start_time: None,
            end_time: None,
        };
        assert!(params.start_time.is_none());
        assert!(params.end_time.is_none());
    }

    #[test]
    fn test_log_level_distribution_response_serialize() {
        let resp = LogLevelDistributionResponse {
            items: vec![LogLevelDistributionItem {
                level: "INFO".to_string(),
                count: 100,
            }],
            total: 100,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("INFO"));
        assert!(json.contains("\"total\":100"));
    }

    #[test]
    fn test_log_time_series_response_serialize() {
        let resp = LogTimeSeriesResponse {
            points: vec![LogTimeSeriesPoint {
                interval_start: 1234567890000,
                count: 42,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("1234567890000"));
    }
}
