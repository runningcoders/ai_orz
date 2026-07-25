//! 日志统计聚合 API DTO

use serde::{Deserialize, Serialize};

/// 日志级别分布项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelDistributionItem {
    /// 日志级别（INFO / WARN / ERROR / DEBUG / TRACE）
    pub level: String,
    /// 数量
    pub count: u64,
}

/// 日志级别分布响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelDistributionResponse {
    /// 分布项列表
    pub items: Vec<LogLevelDistributionItem>,
    /// 总数
    pub total: u64,
}

/// 日志时序数据点（按小时桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTimeSeriesPoint {
    /// 桶起始时间（unix ms）
    pub interval_start: i64,
    /// 该时段日志数
    pub count: u64,
}

/// 日志时序响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTimeSeriesResponse {
    /// 时序数据点列表
    pub points: Vec<LogTimeSeriesPoint>,
}

/// 日志统计查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStatsQueryParams {
    /// 起始时间（unix ms，含），默认 24 小时前
    pub start_time: Option<i64>,
    /// 结束时间（unix ms，含），默认当前
    pub end_time: Option<i64>,
}
