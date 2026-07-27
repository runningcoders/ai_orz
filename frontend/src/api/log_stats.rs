//! 日志统计 API 客户端

use serde::Deserialize;

use super::{ApiError, api_get};

#[derive(Debug, Clone, Deserialize)]
pub struct LogLevelDistributionItem {
    pub level: String,
    pub count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogLevelDistributionResponse {
    pub items: Vec<LogLevelDistributionItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogTimeSeriesPoint {
    pub interval_start: i64,
    pub count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogTimeSeriesResponse {
    pub points: Vec<LogTimeSeriesPoint>,
}

/// 获取日志级别分布（默认最近 24 小时）
pub async fn get_log_level_distribution(
    req: &common::api::LogStatsQueryParams,
) -> Result<LogLevelDistributionResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("start_time", req.start_time.map(|v| v.to_string())),
        ("end_time", req.end_time.map(|v| v.to_string())),
    ]);
    api_get(&format!(
        "/api/v1/system/logs/stats/level-distribution{}",
        qs
    ))
    .await
}

/// 获取日志时序（按小时桶，默认最近 24 小时）
pub async fn get_log_time_series(
    req: &common::api::LogStatsQueryParams,
) -> Result<LogTimeSeriesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("start_time", req.start_time.map(|v| v.to_string())),
        ("end_time", req.end_time.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/system/logs/stats/time-series{}", qs)).await
}
