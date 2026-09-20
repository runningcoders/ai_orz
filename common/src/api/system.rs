//! System 域共享 API DTO - 系统健康指标聚合

use crate::enums::EventTopic;
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
    /// 飞书 WebSocket 长连接监控（无监听时 active_connections=0）
    #[serde(default)]
    pub lark_ws: LarkWsMetrics,
    /// 微信 iLink 入站长轮询监控（无监听时 active_polls=0）
    #[serde(default)]
    pub wechat_poll: WechatPollMetrics,
}

/// 飞书 WebSocket 长连接监控快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LarkWsMetrics {
    /// 活跃监听连接数（per-app 一条）
    pub active_connections: u64,
    /// 每个应用的连接状态明细
    pub apps: Vec<LarkWsAppMetrics>,
}

/// 单个飞书应用的 WS 连接状态
///
/// 判活口径与微信 `WechatPollChannelMetrics` 同构：**只看 `state` 不够**——
/// 「句柄还在、连接看着是 connected」与「真的有帧进来」是两件事（半开连接下
/// 心跳写进内核缓冲区也算成功）。因此必须暴露 `frames_received` 与
/// `last_frame_at_ms`，由页面判断「已连接但帧停走」→ 疑似半开连接。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkWsAppMetrics {
    /// 飞书 App ID
    pub app_id: String,
    /// 连接阶段：connecting / connected / reconnecting / failed
    pub state: String,
    /// 累计重连成功次数（首次建连不计）
    pub reconnect_count: u64,
    /// 累计收到帧数（含控制帧）
    pub frames_received: u64,
    /// 最近一次收到任意帧的时间戳（ms；0 = 本连接从未收到）
    ///
    /// 正常节奏下服务端每 `PingInterval`（默认 120s）会回一次 pong；
    /// 长时间停走且 `state=connected` ⇒ 疑似半开连接。
    pub last_frame_at_ms: i64,
    /// 最近一次 close 帧 code（0 = 未收到 close）
    ///
    /// 服务端主动关闭时唯一能区分「正常轮换」与「连接被顶 / 冲突」的证据。
    pub last_close_code: i64,
    /// 最近一次 close 帧 reason
    #[serde(default)]
    pub last_close_reason: Option<String>,
    /// 终局原因（存在 ⇒ 已停止重连，需人工介入：换凭据 / 排查连接冲突）
    #[serde(default)]
    pub terminal_reason: Option<String>,
}

/// 微信 iLink 入站长轮询监控快照
///
/// 与 [`LarkWsMetrics`] 同构：飞书是服务端推送的 WS 长连接，微信是客户端发起的
/// 长轮询（iLink 无推送通道），两者都需回答同一个问题——「监听现在到底活没活」。
/// 微信侧的判活不能只看「循环在注册表里」，还要看**轮次是否在推进**：
/// 长轮询每轮成功返回都会 `rounds+1` 并刷新 `last_poll_at_ms`，
/// 正常节奏约 35s 一轮，长时间不刷新即卡死。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WechatPollMetrics {
    /// 活跃长轮询数（per-channel 一条）
    pub active_polls: u64,
    /// 每个渠道的轮询运行态明细
    pub channels: Vec<WechatPollChannelMetrics>,
}

/// 单个微信渠道的长轮询运行态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatPollChannelMetrics {
    /// 渠道 ID
    pub channel_id: String,
    /// 渠道名称
    pub channel_name: String,
    /// iLink bot 标识（来自凭证）
    pub bot_id: String,
    /// 轮询阶段：polling（正常）/ degraded（连续失败退避中）/ paused（会话失效暂停中）
    pub state: String,
    /// 累计完成轮次（每轮成功返回 +1）
    pub rounds: u64,
    /// 累计入站消息数（本轮进程生命周期内）
    pub inbound_messages: u64,
    /// 连续失败次数（>0 即当前处于异常；成功一轮归零）
    pub consecutive_failures: u32,
    /// 累计客户端超时次数（>0 表示曾出现网络 hang / 服务端异常）
    pub client_timeouts: u64,
    /// 最近一次成功轮询的时间戳（ms），用于判断「轮询是否卡住」
    pub last_poll_at_ms: i64,
    /// 最近一条入站消息的时间戳（ms；从未收到则为 0）
    pub last_message_at_ms: i64,
    /// 会话暂停解禁时间戳（ms；0 = 未暂停）
    ///
    /// 由服务端 `-14`（会话/令牌失效）触发，暂停该渠道全部请求直至解禁，
    /// 须用户重新扫码授权才能恢复 —— 与 `degraded`（网络抖动，等一会自愈）语义不同。
    pub paused_until_ms: i64,
    /// 已确认消费游标摘要（前 8 字符 + 长度；`None` = 尚未建立进度）
    pub cursor: Option<String>,
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
    /// 事件类型过滤（严格：非法枚举值不会静默当成“无过滤”以外的语义）
    #[param(source = "query")]
    pub event_kind: Option<EventTopic>,
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

/// 触发全量向量索引重建请求（无参数，仅 SuperAdmin）
///
/// 前端拿到返回的 `task_id` 后轮询 `GET /api/v1/system/tasks/{task_id}/progress`
/// 展示进度条。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RebuildVectorsRequest {}

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

// ===== 备份管理 =====

/// 单个备份的元信息
///
/// 协议化改造：原定义在 `src/service/dal/backup.rs`（DAL 内部结构泄漏到 API），
/// 现收敛为前后端共享的唯一定义。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BackupInfo {
    /// 备份版本号（单调递增）
    pub version: u64,
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 归档文件名，例如 `v1_20260717_153000.tar.gz`
    pub file_name: String,
    /// 归档文件字节数
    pub size_bytes: u64,
    /// 归档文件 MD5（十六进制小写）
    pub md5: String,
}

/// 删除备份响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteBackupResponse {
    /// 是否删除成功
    pub success: bool,
}

// ===== 日志查询 =====

/// 单条日志条目
///
/// 协议化改造：原定义在 `src/service/dal/log_query.rs`，现收敛为共享定义。
/// `raw` 为 Option 以兼容前端展开查看（后端总是填充 Some）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LogEntry {
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 日志级别（INFO / WARN / ERROR / DEBUG / TRACE）
    pub level: String,
    /// 日志消息
    pub message: String,
    /// 请求追踪 ID（来自 fields.log_id）
    pub log_id: Option<String>,
    /// 用户 ID（来自 fields.user_id）
    pub user_id: Option<String>,
    /// 操作名称（来自 fields.operation）
    pub operation: Option<String>,
    /// 原始 JSON 对象（用于展开查看完整信息）
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

/// 日志分页查询响应
///
/// 协议化改造：替代原 DAL 的 `LogPageResult`，作为 `GET /api/v1/system/logs` 的标准响应体。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryLogsResponse {
    /// 匹配总数（最多 MAX_SCAN_ENTRIES）
    pub total: usize,
    /// 当前页日志条目
    pub entries: Vec<LogEntry>,
    /// 当前页码
    pub page: usize,
    /// 每页条数
    pub page_size: usize,
}

// ==================== 工具日志存储监控（① 运行时输出层治理）====================

/// 单个日期分区的工具日志占用
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolLogDayStatItem {
    /// 日期目录名（YYYYMMDD）
    pub day: String,
    /// 日志文件数
    pub files: u64,
    /// 占用字节数
    pub bytes: u64,
}

/// GET /api/v1/system/storage/tool-logs 请求
#[derive(Debug, Clone, Default, Serialize, Deserialize, Params)]
pub struct GetToolLogStorageRequest {}

/// GET /api/v1/system/storage/tool-logs 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolLogStorageResponse {
    /// 总占用字节数
    pub total_bytes: u64,
    /// 总文件数
    pub total_files: u64,
    /// 按天占用（升序）
    pub by_day: Vec<ToolLogDayStatItem>,
    /// 保留天数配置（0 = 不自动清理；修改 ai_orz.toml [tool_log].retention_days 后重启生效）
    pub retention_days: u32,
}

/// POST /api/v1/system/storage/tool-logs/cleanup 请求
#[derive(Debug, Clone, Default, Serialize, Deserialize, Params)]
pub struct CleanupToolLogsRequest {
    /// 本次清理使用的保留天数（缺省读 [tool_log].retention_days 配置；0 = 清理关闭，空跑）
    #[serde(default)]
    pub retention_days: Option<u32>,
}

/// POST /api/v1/system/storage/tool-logs/cleanup 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CleanupToolLogsResponse {
    /// 是否执行了清理（retention = 0 时 false，表示清理关闭空跑）
    pub success: bool,
    /// 使用的保留天数
    pub retention_days: u32,
    /// 删除的日期目录数
    pub removed_dirs: u64,
    /// 删除的日志文件数
    pub removed_files: u64,
    /// 释放的字节数
    pub freed_bytes: u64,
    /// 因 Running 进程保护跳过的目录数
    pub skipped_dirs: u64,
}

// ==================== 工作台顶栏聚合指标 ====================

/// 工作台顶栏聚合指标请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetWorkspaceMetricsRequest {
    /// 模型/工具统计窗口（分钟），缺省 60，上限 1440
    #[param(source = "query")]
    pub minutes: Option<u32>,
}

/// 工作台顶栏聚合指标响应
///
/// 单一端点覆盖顶栏全部数字指标（项目/Agent 概览 + 运行态三色计数 +
/// 模型/工具窗口读数 + AOP 队列积压），前端一个 30 秒轮询即可拿全量，
/// 保证所有数字出自同一份快照；各维度独立降级为 0，单维度故障不影响整体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceMetricsResponse {
    /// 项目总数（DAO 默认排除 Deleted）
    pub project_count: u64,
    /// Agent 总数（排除 Deleted）
    pub agent_count: u64,
    /// 运行中项目数（status 1..=3，与前端 is_active_project 同口径）
    pub active_project_count: u64,
    /// 运行态 Agent 三色计数（内存实时，未注册运行的 Agent 不计入）
    pub runtime: AgentRuntimeCounts,
    /// 模型/工具统计窗口（分钟）
    pub window_minutes: u32,
    /// 模型调用读数（窗口内；批次刷盘，最近 1~2 分钟数据可能未落库）
    pub model: ModelUsageMetrics,
    /// 工具调用读数（窗口内；同上）
    pub tool: ToolUsageMetrics,
    /// AOP 队列总待处理数（所有消费者累加，内存实时）
    pub queue_backlog: u64,
}

/// 运行态 Agent 三色计数
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AgentRuntimeCounts {
    /// 空闲
    pub idle: u64,
    /// 忙碌（顶栏「忙碌」概览卡与「思考」点同源口径）
    pub busy: u64,
    /// 休息
    pub resting: u64,
}

/// 模型调用窗口读数
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ModelUsageMetrics {
    /// 调用次数
    pub total_calls: u64,
    /// 输入 Token 合计
    pub total_tokens_input: u64,
    /// 输出 Token 合计
    pub total_tokens_output: u64,
}

/// 工具调用窗口读数
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ToolUsageMetrics {
    /// 调用次数
    pub total_calls: u64,
    /// 失败次数
    pub failed_calls: u64,
    /// 平均耗时（毫秒），窗口内无调用时为 None
    pub avg_duration_ms: Option<f64>,
}
