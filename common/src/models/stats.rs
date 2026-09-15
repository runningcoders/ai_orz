//! Common statistics-related models.
//!
//! These types are shared across all layers (DAO/DAL/Domain/API) for stats query results.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Time series interval for grouping data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum StatsInterval {
    /// Group by minute（分钟级，用于实时 QPS 曲线）
    Minutely,
    /// Group by hour
    Hourly,
    /// Group by day
    Daily,
}

/// 时序查询返回桶数的上限（策略护栏）
///
/// 「窗口跨度 × 聚合粒度」由请求侧自由组合，若不设上限，`Minutely` 配 30 天窗口
/// 会产出 43200 行结果：既浪费聚合与序列化开销，前端也画不出可读的折线。
/// 这里统一按**桶数**收敛，超过上限时逐级回退到更粗的粒度
/// （配合 [`clamp_interval_to_span`]）。
///
/// 取值依据：约为单张折线图可读点数的量级上界；对现有正常请求零影响
/// （侧栏 60 个分钟桶、详情页 30 个天桶都远低于它）。
pub const STATS_MAX_BUCKETS: i64 = 512;

impl StatsInterval {
    /// 单桶时长（毫秒）
    pub const fn bucket_ms(self) -> i64 {
        match self {
            Self::Minutely => 60_000,
            Self::Hourly => 3_600_000,
            Self::Daily => 86_400_000,
        }
    }

    /// 窗口 `span_ms` 在该粒度下的桶数上界（`span_ms <= 0` 时记 0，表示无法判定）
    pub const fn bucket_count(self, span_ms: i64) -> i64 {
        if span_ms <= 0 {
            0
        } else {
            span_ms / self.bucket_ms()
        }
    }
}

/// 粒度阶梯：由细到粗，收敛时只会沿这个方向回退
const INTERVAL_LADDER: [StatsInterval; 3] = [
    StatsInterval::Minutely,
    StatsInterval::Hourly,
    StatsInterval::Daily,
];

/// 按窗口跨度收敛聚合粒度，保证桶数不超过 [`STATS_MAX_BUCKETS`]
///
/// 返回**最细但不超限**的粒度：请求粒度本身不超限时原样返回，否则沿粒度阶梯
/// 逐级向粗档回退（`Minutely` → `Hourly` → `Daily`，`Daily` 为最终兜底）。
///
/// `span_ms <= 0`（窗口缺失或非法）时不干预 —— 代价无从判断，交回调用方
/// 既有的默认窗口兜底。
///
/// 这是服务端护栏，与前端「按跨度挑粒度」的启发式互为独立的两道防线：
/// 前端可以不发超限请求，但无法阻止手写 query 的调用方，故后端必须自兜底。
pub fn clamp_interval_to_span(interval: StatsInterval, span_ms: i64) -> StatsInterval {
    if span_ms <= 0 {
        return interval;
    }
    let requested = INTERVAL_LADDER
        .iter()
        .position(|tier| *tier == interval)
        .unwrap_or(INTERVAL_LADDER.len() - 1);
    // Daily 兜底档在任何有限窗口下都满足上限，`find` 不会落空
    let affordable = INTERVAL_LADDER
        .iter()
        .position(|tier| tier.bucket_count(span_ms) <= STATS_MAX_BUCKETS)
        .unwrap_or(INTERVAL_LADDER.len() - 1);
    INTERVAL_LADDER[requested.max(affordable)]
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
///
/// 与 [`ModelCallStats`] 对位：同样是「领域通用统计结构体」，
/// 既可挂在单个 Tool 上（`tool.stats`），也可用于组织级汇总读数
/// （`ToolStatsQuery.tool_id = None`，不按工具收窄）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ToolStats {
    /// 调用次数汇总（次数 + QPS）
    pub call_summary: Option<CallSummary>,
    /// 失败次数
    pub failed_count: Option<u64>,
    /// 平均调用耗时（毫秒），窗口内无调用时为 None
    pub avg_duration_ms: Option<f64>,
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

/// 项目进度汇总（实时计算，不持久化）
///
/// 由 Domain 层根据项目关联的任务列表实时聚合，
/// 通过 `GetProjectRequest.with_progress_summary=true` 按需返回。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ProjectProgressSummary {
    /// 任务总数
    pub total_tasks: usize,
    /// 已完成数
    pub completed: usize,
    /// 进行中数
    pub in_progress: usize,
    /// 待启动数（Pending + PendingReview）
    pub pending: usize,
    /// 阻塞数（预留，目前固定为 0）
    pub blocked: usize,
    /// 已取消数
    pub cancelled: usize,
    /// 整体进度百分比（0-100，任务进度均值）
    pub overall_percent: u32,
}
