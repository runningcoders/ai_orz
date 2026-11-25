//! Cron Trigger related API request/response DTOs - shared between backend and frontend

use crate::enums::{CreatorType, ScheduleDateTerm, TriggerBehavior, TriggerType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create Cron Trigger request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateCronTriggerRequest {
    /// Trigger name
    pub name: String,
    /// Trigger type
    pub trigger_type: TriggerType,
    /// Cron expression (required for Cron type)
    pub cron_expression: Option<String>,
    /// Interval in seconds (required for Interval type)
    pub interval_seconds: Option<i64>,
    /// Run at timestamp (required for Once type)
    pub run_at: Option<i64>,
    /// Trigger payload JSON string
    pub payload: String,
}

/// Update Cron Trigger request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
    /// New trigger name
    pub name: Option<String>,
    /// New trigger type
    pub trigger_type: Option<TriggerType>,
    /// New cron expression
    pub cron_expression: Option<String>,
    /// New interval in seconds
    pub interval_seconds: Option<i64>,
    /// New run at timestamp
    pub run_at: Option<i64>,
    /// New trigger payload JSON string
    pub payload: Option<String>,
}

/// Cron Trigger detail
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronTriggerDetail {
    /// Trigger ID
    pub id: String,
    /// Trigger name
    pub name: String,
    /// Trigger type
    pub trigger_type: TriggerType,
    /// Cron expression (for Cron type)
    pub cron_expression: Option<String>,
    /// Interval in seconds (for Interval type)
    pub interval_seconds: Option<i64>,
    /// Run at timestamp (for Once type)
    pub run_at: Option<i64>,
    /// Next run at timestamp
    pub next_run_at: i64,
    /// Whether the trigger is enabled
    pub is_enabled: bool,
    /// Trigger payload JSON string
    pub payload: String,
    /// Last run at timestamp
    pub last_run_at: Option<i64>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// Creator user ID
    pub created_by: Option<String>,
    /// Last modifier user ID
    pub updated_by: Option<String>,
    /// When: effective window start (ms)
    #[serde(default)]
    pub start_at: Option<i64>,
    /// When: effective window end (ms)
    #[serde(default)]
    pub end_at: Option<i64>,
    /// Who: organization id
    #[serde(default)]
    pub org_id: Option<String>,
    /// Who: root user id (audience)
    #[serde(default)]
    pub owner_user_id: Option<String>,
    /// Who: 创建者类型（用户 / Agent / 系统）
    #[serde(default)]
    pub creator_type: Option<CreatorType>,
    /// Who: 创建者 ID（`creator_type` 解释其所属表）
    #[serde(default)]
    pub creator_id: Option<String>,
    /// Who: 负责该日程的 Agent ID（到点唤醒它，或由它发出提醒）
    #[serde(default)]
    pub agent_id: Option<String>,
    /// What: human-readable description / schedule instruction
    #[serde(default)]
    pub description: String,
    /// What: business action tag (falls back to payload.action for legacy rows)
    #[serde(default)]
    pub action: Option<String>,
    /// How: behavior template
    #[serde(default)]
    pub behavior: TriggerBehavior,
    /// How: max run count (None = unlimited)
    #[serde(default)]
    pub max_runs: Option<i64>,
    /// How: executed run count
    #[serde(default)]
    pub total_runs: i64,
    /// How: last execution error
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Agent 自主创建日程的请求（handler 工具 `schedule_agent_task`）
///
/// ⚠️ 不接受 `org_id` / `owner_user_id` / `creator_type` / `creator_id`：
/// 这些是**身份**，由 handler 从 `RequestContext` 固化，模型传入即等于越权。
/// `agent_id`（负责 Agent）是业务字段而非身份，允许显式指定，缺省为创建者自己
/// （仅当创建者是 Agent）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ScheduleAgentTaskRequest {
    /// 日程标题（同时作为触发器名称与唤醒正文的主语）
    pub title: String,
    /// 用户原始意图 / 详细说明：到点唤醒时原样回灌给 Agent
    pub description: String,
    /// 时间规则：cron 表达式（与 interval_seconds / run_at 三选一，优先级 cron > interval > run_at）
    pub cron_expression: Option<String>,
    /// 时间规则：间隔秒数
    pub interval_seconds: Option<i64>,
    /// 时间规则：一次性触发的 Unix 毫秒时间戳
    pub run_at: Option<i64>,
    /// 生效窗口开始（Unix 毫秒）。更新时传 null 表示清除
    pub start_at: Option<i64>,
    /// 生效窗口结束（Unix 毫秒）。更新时传 null 表示清除
    pub end_at: Option<i64>,
    /// 负责该日程的 Agent ID；缺省 = 创建者自己（创建者必须是 Agent）
    pub agent_id: Option<String>,
    /// 行为模板：缺省 `AsyncAgentWakeup`；只允许 AsyncNotify / AsyncAgentWakeup
    pub behavior: Option<TriggerBehavior>,
    /// 动作标签（写顶层 action 列）：缺省用 title
    pub action: Option<String>,
    /// 最大触发次数（必须为正数）；None = 不限
    pub max_runs: Option<i32>,
}

/// Agent 自主创建日程的响应（复用统一触发器详情）
pub type ScheduleAgentTaskResponse = CronTriggerDetail;

/// 日程查询请求（含相对日期词 + 可选显式区间）
///
/// `date_term` 与 `from`/`to` 二者至少给其一；给 `from`/`to` 时按自定义区间查询
/// （此时 `date_term` 仅作响应回显）。相对日词由服务端按 `server.timezone` 折算，
/// **模型不要自己算毫秒时间戳**。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryScheduleRequest {
    /// 相对日期词：yesterday / today / tomorrow / this_week / next_week
    pub date_term: Option<ScheduleDateTerm>,
    /// 自定义区间起点（Unix 毫秒，含）
    pub from: Option<i64>,
    /// 自定义区间终点（Unix 毫秒，不含）
    pub to: Option<i64>,
    /// 关键词：匹配名称、描述、动作标签（大小写不敏感）
    pub keyword: Option<String>,
    /// 按行为模板筛选
    pub behavior: Option<TriggerBehavior>,
    /// 仅看启用/停用
    pub is_enabled: Option<bool>,
    /// 返回条数上限（默认 200，硬上限 500）
    pub limit: Option<usize>,
}

/// 日程实例：一条重复规则在某个时间点的一次展开结果
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleOccurrence {
    /// 规则 ID
    pub trigger_id: String,
    /// 本次实例的发生时间（Unix 毫秒）
    pub run_at: i64,
    /// 规则名称（日程标题）
    pub name: String,
    /// 用户原始意图 / 说明
    pub description: String,
    /// 业务动作标签
    pub action: Option<String>,
    /// 行为模板
    pub behavior: TriggerBehavior,
    /// 实际生效的时间规则（Schedule 形态下为 Once/Cron/Interval）
    pub rule_kind: TriggerType,
    /// 该实例是否重复序列中的一次（Once 为 false）
    pub recurring: bool,
    /// Who: 创建者类型（谁安排的）
    #[serde(default)]
    pub creator_type: Option<CreatorType>,
    /// Who: 创建者 ID（`creator_type` 解释其所属表）
    #[serde(default)]
    pub creator_id: Option<String>,
    /// Who: 负责 Agent（谁来干）
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Who: 归属用户（算谁的 / 推给谁）
    #[serde(default)]
    pub owner_user_id: Option<String>,
}

/// 日程查询响应：展开后的实例列表 + 本次解析出的区间
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct QueryScheduleResponse {
    /// 实际使用的日期词
    pub date_term: ScheduleDateTerm,
    /// 系统时区（IANA）
    pub timezone: String,
    /// 查询区间起点（含，Unix 毫秒）
    pub from: i64,
    /// 查询区间终点（不含，Unix 毫秒）
    pub to: i64,
    /// 展开后的实例（按 run_at 升序）
    pub occurrences: Vec<ScheduleOccurrence>,
}

/// List Cron Triggers response
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListCronTriggersResponse {
    /// List of triggers
    pub triggers: Vec<CronTriggerDetail>,
    /// Total count
    pub total: usize,
}

/// Cron Trigger list item alias (frontend compatibility)
pub type ListCronTriggersResponseItem = CronTriggerDetail;

/// Create Cron Trigger response
pub type CreateCronTriggerResponse = CronTriggerDetail;

/// Get Cron Trigger response
pub type GetCronTriggerResponse = CronTriggerDetail;

/// Update Cron Trigger response
pub type UpdateCronTriggerResponse = CronTriggerDetail;

/// Delete Cron Trigger response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteCronTriggerResponse {
    /// Whether deletion succeeded
    pub success: bool,
}

/// Get Cron Trigger request (path id)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
}

/// List Cron Triggers request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListCronTriggersRequest {
    /// Filter by trigger type
    #[param(source = "query")]
    pub trigger_type: Option<TriggerType>,
    /// Filter by enabled state
    #[param(source = "query")]
    pub is_enabled: Option<bool>,
    /// Max result count
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// Delete Cron Trigger request (path id)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
}

/// Agent 取消自己创建的日程（软删；幂等）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CancelScheduleRequest {
    /// 目标日程 ID
    pub trigger_id: String,
}

/// Agent 取消日程的响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelScheduleResponse {
    /// 本次是否真的取消了（false 表示此前已取消，幂等 no-op，不是错误）
    pub cancelled: bool,
    /// 说明信息
    pub message: String,
}

/// Agent 更新自己创建的日程（只开放安全字段）
///
/// 时间窗口与次数用**三态**表达：
/// - 字段缺省 → 保持原值
/// - 显式传 `null` → 清除（窗口变无限制 / 次数变不限）
/// - 传具体值 → 覆盖
///
/// 单层 `Option` 无法区分「没传」与「清空」，会让用户永远无法取消 `max_runs`
/// 或窗口限制。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct UpdateScheduleRequest {
    /// 目标日程 ID
    pub trigger_id: String,
    /// 新标题
    pub title: Option<String>,
    /// 新描述（到点唤醒时回灌给 Agent 的完整意图）
    pub description: Option<String>,
    /// 新 cron 表达式（5 字段）
    pub cron_expression: Option<String>,
    /// 新间隔秒数
    pub interval_seconds: Option<i64>,
    /// 新一次性触发时刻（Unix 毫秒）
    pub run_at: Option<i64>,
    /// 新生效窗口起点（Unix 毫秒）；`null` 表示清除
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<Option<i64>>,
    /// 新生效窗口终点（Unix 毫秒）；`null` 表示清除
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<Option<i64>>,
    /// 新最大触发次数（必须为正数）；`null` 表示清除限制
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<Option<i32>>,
    /// 新负责 Agent ID（改派执行者）
    pub agent_id: Option<String>,
}

/// Agent 更新日程的响应（复用统一触发器详情）
pub type UpdateScheduleResponse = CronTriggerDetail;

/// Pause Cron Trigger request (path id)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct PauseCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
}

/// Resume Cron Trigger request (path id)
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ResumeCronTriggerRequest {
    /// Trigger ID
    #[param(source = "path")]
    pub trigger_id: String,
}

/// Pause Cron Trigger response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PauseCronTriggerResponse {
    /// Whether pause succeeded
    pub success: bool,
}

/// Resume Cron Trigger response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResumeCronTriggerResponse {
    /// Whether resume succeeded
    pub success: bool,
}
