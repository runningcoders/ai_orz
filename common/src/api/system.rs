//! System 域共享 API DTO - 系统健康指标聚合

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 系统健康指标聚合响应
///
/// 用于前端 HUD 仪表盘墙展示，由后端 `GET /api/v1/system/health/metrics` 返回。
/// 部分维度允许降级为 0（跨域获取成本高时）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetricsResponse {
    /// 后端服务在线（handler 能响应即视为 true）
    pub backend_online: bool,
    /// AOP 队列总待处理数（所有消费者累加）
    pub aop_pending: u64,
    /// AOP 队列总处理中数（所有消费者累加）
    pub aop_in_progress: u64,
    /// 活跃 Agent 数（status != 0），降级为 0
    pub active_agents: u64,
    /// 总 Agent 数，降级为 0
    pub total_agents: u64,
    /// 活跃项目数，降级为 0
    pub active_projects: u64,
    /// 总项目数，降级为 0
    pub total_projects: u64,
    /// 待处理任务数（status != Done），降级为 0
    pub pending_tasks: u64,
    /// 总任务数，降级为 0
    pub total_tasks: u64,
    /// 进程运行时长（秒）
    pub uptime_secs: u64,
}

/// 创建备份请求（无参数，由 Admin 触发）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateBackupRequest {}

/// 列出备份请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListBackupsRequest {}

/// 删除备份请求（path 参数：version）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteBackupRequest {
    /// 备份版本号
    #[param(source = "path")]
    pub version: u64,
}

/// 系统健康指标聚合请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetHealthMetricsRequest {}

// ============ AOP Queue Monitoring ============

/// AOP 队列统计概览请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAllQueueStatsRequest {}

/// AOP 单消费者队列统计请求（path: consumer）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetQueueStatsRequest {
    /// 消费者名称
    #[param(source = "path")]
    pub consumer: String,
}

/// AOP 事件列表查询参数
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListEventsRequest {
    /// 消费者名称
    #[param(source = "path")]
    pub consumer: String,
    /// 排序键
    #[param(source = "query")]
    pub order_key: Option<String>,
    /// 状态过滤
    #[param(source = "query")]
    pub status: Option<String>,
    /// 返回数量限制
    #[param(source = "query")]
    pub limit: Option<usize>,
    /// 偏移量
    #[param(source = "query")]
    pub offset: Option<usize>,
}

/// AOP 单事件详情请求（path: consumer + event_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetEventRequest {
    /// 消费者名称
    #[param(source = "path")]
    pub consumer: String,
    /// 事件 ID
    #[param(source = "path")]
    pub event_id: String,
}

// ============ AOP Realtime Stats ============

/// AOP 实时统计概览请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetStatsOverviewRequest {}

/// AOP 实时统计时序查询参数
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetStatsTimeSeriesRequest {
    /// 事件类型过滤
    #[param(source = "query")]
    pub event_kind: Option<String>,
    /// 消费者名称过滤
    #[param(source = "query")]
    pub consumer_name: Option<String>,
    /// 状态过滤
    #[param(source = "query")]
    pub status: Option<String>,
}

/// AOP 实时统计分布查询参数
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetStatsDistributionRequest {
    /// 分组维度：consumer / status / kind
    #[param(source = "query")]
    pub group_by: String,
    /// 状态过滤
    #[param(source = "query")]
    pub status: Option<String>,
}

// ============ AOP Response Types ============

/// AOP 队列统计响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueueStatsResponse {
    /// 消费者名称
    pub consumer_name: String,
    /// 待处理数
    pub pending_count: usize,
    /// 处理中数
    pub in_progress_count: usize,
    /// 排序键信息
    pub order_keys: Vec<OrderKeyInfo>,
    /// 最老事件年龄（秒）
    pub oldest_event_age_secs: Option<u64>,
}

/// 排序键信息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrderKeyInfo {
    /// 排序键
    pub order_key: String,
    /// 待处理数
    pub pending_count: usize,
}

/// AOP 事件摘要响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventSummaryResponse {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_kind: String,
    /// 排序键
    pub order_key: String,
    /// 优先级
    pub priority: u8,
    /// 创建时间戳
    pub created_at: i64,
    /// 状态
    pub status: String,
}

/// AOP 事件详情响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventDetailResponse {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_kind: String,
    /// 排序键
    pub order_key: String,
    /// 优先级
    pub priority: u8,
    /// 创建时间戳
    pub created_at: i64,
    /// 状态
    pub status: String,
    /// payload 预览
    pub payload_preview: String,
}

/// AOP 实时统计概览响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AopStatsOverviewResponse {
    /// 总发布数
    pub total_published: u64,
    /// 总消费数
    pub total_consumed: u64,
    /// 总成功数
    pub total_success: u64,
    /// 总失败数
    pub total_failed: u64,
    /// 平均耗时（毫秒）
    pub avg_duration_ms: f64,
}

/// AOP 实时统计时序数据点
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AopStatsTimeSeriesPoint {
    /// 桶起始时间
    pub interval_start: i64,
    /// 调用数
    pub call_count: u64,
}

/// AOP 实时统计时序响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AopStatsTimeSeriesResponse {
    /// 数据点列表
    pub points: Vec<AopStatsTimeSeriesPoint>,
}

/// AOP 实时统计分布项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AopStatsDistributionItem {
    /// 标签
    pub label: String,
    /// 数量
    pub value: u64,
}

/// AOP 实时统计分布响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AopStatsDistributionResponse {
    /// 分布项列表
    pub items: Vec<AopStatsDistributionItem>,
}

// ============ Process Management (shell_list / shell_status / shell_kill) ============

/// 列出后台进程请求（无参数，可见范围由调用方 ctx 决定）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListProcessesRequest {}

/// 后台进程概要信息（列表项）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessInfo {
    /// 进程 pid
    pub pid: u32,
    /// 关联的工具调用 call_id
    pub call_id: String,
    /// 启动该进程的工具 ID
    pub tool_id: String,
    /// 启动方 Agent ID（人类/管理面启动为 None）
    pub agent_id: Option<String>,
    /// 执行的命令
    pub command: String,
    /// 工作目录
    pub working_dir: String,
    /// 是否后台启动
    pub background: bool,
    /// 启动时间戳（ms）
    pub started_at: u64,
    /// 是否存活（返回前探活刷新）
    pub alive: bool,
    /// 退出码（已退出且可得时）
    pub exit_code: Option<i32>,
    /// 输出日志路径
    pub log_path: String,
}

/// 列出后台进程响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListProcessesResponse {
    /// 进程列表（按启动时间升序）
    pub processes: Vec<ProcessInfo>,
}

/// 查询后台进程状态请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ShellStatusRequest {
    /// 进程 pid
    #[param(source = "path")]
    pub pid: u32,
    /// 返回日志尾部行数（默认 20，上限 500）
    #[param(source = "query")]
    pub tail_lines: Option<usize>,
}

/// 后台进程状态响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShellStatusResponse {
    /// 进程 pid
    pub pid: u32,
    /// 是否存活
    pub alive: bool,
    /// 退出码（已退出且可得时）
    pub exit_code: Option<i32>,
    /// 启动时间戳（ms）
    pub started_at: u64,
    /// 执行的命令
    pub command: String,
    /// 输出日志路径
    pub log_path: String,
    /// 关联的工具调用 call_id
    pub call_id: String,
    /// 日志尾部
    pub log_tail: String,
}

/// 终止后台进程请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ShellKillRequest {
    /// 进程 pid
    #[param(source = "path")]
    pub pid: u32,
}

/// 终止后台进程响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShellKillResponse {
    /// 进程 pid
    pub pid: u32,
    /// 是否实际执行了终止（进程已退出时为 false）
    pub killed: bool,
}
