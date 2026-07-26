//! 日志统计聚合 API DTO

use ai_orz_macros::Params;
use schemars::JsonSchema;
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Params)]
pub struct LogStatsQueryParams {
    /// 起始时间（unix ms，含），默认 24 小时前
    #[param(source = "query")]
    pub start_time: Option<i64>,
    /// 结束时间（unix ms，含），默认当前
    #[param(source = "query")]
    pub end_time: Option<i64>,
}

/// 应用日志查询请求（query 参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LogQueryRequest {
    /// 关键词（message 字段包含，不区分大小写）
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 调用链 ID 精确匹配
    #[param(source = "query")]
    pub log_id: Option<String>,
    /// 日志级别过滤（INFO / WARN / ERROR / DEBUG）
    #[param(source = "query")]
    pub level: Option<String>,
    /// 起始时间（unix timestamp ms，含）
    #[param(source = "query")]
    pub start_time: Option<i64>,
    /// 结束时间（unix timestamp ms，含）
    #[param(source = "query")]
    pub end_time: Option<i64>,
    /// 页码（从 1 开始，默认 1）
    #[param(source = "query")]
    pub page: Option<usize>,
    /// 每页条数（默认 20）
    #[param(source = "query")]
    pub page_size: Option<usize>,
}
