//! 日志统计 API 客户端
//!
//! 协议化改造：DTO 全部复用 `common::api` 共享定义（此前本地镜像了一份，存在漂移风险），
//! 此处 re-export 保持既有 `crate::api::log_stats::*` 导入路径可用。

pub use common::api::{
    LogLevelDistributionItem, LogLevelDistributionResponse, LogTimeSeriesPoint,
    LogTimeSeriesResponse,
};

use super::{ApiError, api_get};

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
