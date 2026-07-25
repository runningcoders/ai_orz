//! Common statistics-related models.
//!
//! These types are shared across all layers (DAO/DAL/Domain/API) for stats query results.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Time series interval for grouping data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum StatsInterval {
    /// Group by hour
    Hourly,
    /// Group by day
    Daily,
}

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TimeSeriesPoint {
    /// Start timestamp of this interval (millis)
    pub interval_start: i64,
    /// Total input tokens
    pub tokens_input: u64,
    /// Total output tokens
    pub tokens_output: u64,
    /// Number of calls
    pub call_count: u64,
}

/// Total token sum result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TokenSumResult {
    /// Total input tokens
    pub total_tokens_input: u64,
    /// Total output tokens
    pub total_tokens_output: u64,
    /// Total number of calls
    pub total_calls: u64,
}

/// 调用次数汇总（最通用的统计结果）
///
/// 任何实体被调用都可以用这个结构体表示。
/// QPS 分为两种：
/// - `avg_qps`: 平均 QPS，需要传入 time_range 才能计算
/// - `instant_qps`: 瞬时 QPS，按最近 1 秒调用次数统计
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CallSummary {
    /// 总调用次数
    pub total_calls: u64,
    /// 平均 QPS（每秒查询率），需要 time_range 才有值
    pub avg_qps: Option<f64>,
    /// 瞬时 QPS（最近 1 秒调用次数）
    pub instant_qps: f64,
}

/// 统计数据获取选项
///
/// 通过布尔标志控制需要填充哪些维度，避免不必要的查询。
#[derive(Debug, Clone, Default)]
pub struct StatsFetchOptions {
    /// 是否获取调用次数汇总（CallSummary）
    pub with_call_summary: bool,
    /// 是否获取 Token 汇总（TokenSumResult）
    pub with_token_summary: bool,
    /// 是否获取时序数据（Vec<TimeSeriesPoint>）
    pub with_time_series: bool,
    /// 时间范围（毫秒），None 表示全部历史
    pub time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub interval: Option<StatsInterval>,
}

// ==================== 领域统计结构体 ====================
//
// 按领域划分，不同领域的统计结构体职责单一，互不交叉：
//
// - 实体自身统计（AgentStats/ProjectStats/TaskStats）：只关注实体自身维度
//   目前只有 call_summary，未来有了专属统计表可以扩展更多字段
//
// - 模型调用统计（ModelCallStats）：模型调用领域的通用统计结构体
//   所有实体（Agent/Project/Task/ModelProvider）的模型调用统计都用这个结构体
//   由 ModelProviderStatsDao 负责计算，各实体 DAL 层按需组装

/// Agent 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct AgentStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
    /// 工具调用分布（按 tool 维度聚合，由 tool_call_events 表统计而来）
    pub tool_call_summary: Option<ToolCallSummary>,
}

/// 工具调用分布单项（按 tool_id 分组聚合结果）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ToolCallCount {
    /// 工具 ID
    pub tool_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 该工具被调用次数
    pub count: u64,
}

/// 工具调用分布汇总
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ToolCallSummary {
    /// 总调用次数（所有工具合计）
    pub total_calls: u64,
    /// 按 tool 维度分组（按 count 降序）
    pub by_tool: Vec<ToolCallCount>,
}

/// Project 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ProjectStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
}

/// Task 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct TaskStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
}

/// 工具自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ToolStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
    /// 失败次数
    pub failed_count: Option<u64>,
}

/// 模型调用统计（通用，所有实体共用）
///
/// 由 ModelProviderStatsDao 负责计算，
/// 支持按 agent_id / project_id / task_id / model_provider_id 过滤。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ModelCallStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
    /// Token 汇总（模型调用特有）
    pub token_summary: Option<TokenSumResult>,
    /// 模型调用时序趋势
    pub model_call_time_series: Option<Vec<TimeSeriesPoint>>,
}
