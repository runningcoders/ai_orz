//! 日志统计 API 客户端

use serde::Deserialize;

use super::{api_get, ApiError};

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
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Result<LogLevelDistributionResponse, ApiError> {
    let mut params = Vec::new();
    if let Some(s) = start_time {
        params.push(format!("start_time={}", s));
    }
    if let Some(e) = end_time {
        params.push(format!("end_time={}", e));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        "/api/v1/system/logs/stats/level-distribution".to_string()
    } else {
        format!("/api/v1/system/logs/stats/level-distribution?{}", qs)
    };
    api_get(&path).await
}

/// 获取日志时序（按小时桶，默认最近 24 小时）
pub async fn get_log_time_series(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Result<LogTimeSeriesResponse, ApiError> {
    let mut params = Vec::new();
    if let Some(s) = start_time {
        params.push(format!("start_time={}", s));
    }
    if let Some(e) = end_time {
        params.push(format!("end_time={}", e));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        "/api/v1/system/logs/stats/time-series".to_string()
    } else {
        format!("/api/v1/system/logs/stats/time-series?{}", qs)
    };
    api_get(&path).await
}
