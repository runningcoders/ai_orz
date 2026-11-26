//! Cron Trigger related API request/response DTOs - shared between backend and frontend

use crate::enums::{CreatorType, ScheduleDateTerm, TriggerBehavior, TriggerStatus, TriggerType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 系统级同步维护动作模板。
///
/// 与 `consumer/scheduler.rs::dispatch_sync_maintenance` 的 6 个分支一一对应：
/// 前端表单据此渲染可选项并生成 payload，后端 seed 据此创建默认触发器。
/// 放 common 是因为两端都要用——各写一份字符串常量会漂移，选错 action 到点只会告警。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemActionTemplate {
    /// `payload.action` 的值（也是顶层 action 列的取值）
    pub action: &'static str,
    /// 表单下拉展示名
    pub label: &'static str,
    /// 这个动作到点做什么
    pub description: &'static str,
    /// 是否需要额外指定 Agent（当前仅 `agent_rest` 支持，留空表示作用于全部）
    pub needs_agent: bool,
}

/// 后端真实支持的系统维护动作，顺序即表单展示顺序
pub const SYSTEM_ACTION_TEMPLATES: &[SystemActionTemplate] = &[
    SystemActionTemplate {
        action: "agent_rest",
        label: "Agent 睡眠沉淀",
        description: "派发 Agent 的短期记忆沉淀请求；留空则扫描全部待沉淀 Agent",
        needs_agent: true,
    },
    SystemActionTemplate {
        action: "project_followup",
        label: "项目进度巡检",
        description: "扫描进行中的项目，推进停滞任务",
        needs_agent: false,
    },
    SystemActionTemplate {
        action: "tool_log_cleanup",
        label: "工具日志清理",
        description: "清理超过保留期的工具运行日志",
        needs_agent: false,
    },
    SystemActionTemplate {
        action: "directory_reconcile",
        label: "组织目录对账",
        description: "与对端组织双向对账成员目录",
        needs_agent: false,
    },
    SystemActionTemplate {
        action: "pending_notification_redelivery",
        label: "滞留通知重投",
        description: "重投停留在 Pending 的跟进通知",
        needs_agent: false,
    },
    SystemActionTemplate {
        action: "vector_maintenance",
        label: "向量库整理",
        description: "合并向量库碎片并回收旧版本",
        needs_agent: false,
    },
];

/// 按 action id 查模板；`None` 表示不在系统维护动作集合内（自定义 / 历史值）。
pub fn system_action_template(action: &str) -> Option<&'static SystemActionTemplate> {
    SYSTEM_ACTION_TEMPLATES
        .iter()
        .find(|template| template.action == action)
}

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
    /// 日程信息 / 备注（自由 JSON）：Agent 写给自己（或负责 Agent）的内容，只有它自己读。
    ///
    /// 框架在这里**没有任何协议**：字段、结构、含义全由 Agent 自定义，服务端不解析、
    /// 不校验、不解释；到点原样附在唤醒正文里回灌，由 Agent 自己决定拿来做什么。
    /// 日程不存在「动作」语义——本字段与内置触发器的 action 分派协议无关，
    /// 也不是顶层 action 展示标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
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
    /// 展示 / 检索用的日程标签（写顶层 action 列）：缺省用 title；不参与执行分派
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
    /// 关键词：匹配名称、描述、日程标签（大小写不敏感）
    pub keyword: Option<String>,
    /// 按行为模板筛选
    pub behavior: Option<TriggerBehavior>,
    /// 是否连已取消（停用）的日程一起返回。
    ///
    /// 默认 `false`：只返回正常可用的日程，Agent 绝大多数查询无需拼这个参数。
    /// 只有明确要看「曾经取消过什么」时才置 `true`。
    #[serde(default)]
    pub include_cancelled: bool,
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
    /// 展示 / 检索用日程标签（缺省为标题），不表示任何动作
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
    /// 该日程当前是否正常可用（false = 已取消 / 停用）；
    /// 仅在 `include_cancelled=true` 时才会出现 false 的条目。
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

fn default_true() -> bool {
    true
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
///
/// 过滤全部在数据端完成：状态 / 行为都是列表参数（逗号分隔），分页用 offset/limit。
/// `statuses` 缺省只返回启用项；要同时看取消态必须显式传两个状态。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListCronTriggersRequest {
    /// Filter by trigger type
    #[param(source = "query")]
    pub trigger_type: Option<TriggerType>,
    /// 状态列表（enabled / disabled，逗号分隔）；缺省只看 enabled
    #[param(source = "query")]
    #[serde(default, deserialize_with = "crate::serde_ext::deserialize_comma_list")]
    pub statuses: Option<Vec<TriggerStatus>>,
    /// 行为模板列表（Serde 枚举名，逗号分隔）；不传表示全部
    #[param(source = "query")]
    #[serde(default, deserialize_with = "crate::serde_ext::deserialize_comma_list")]
    pub behaviors: Option<Vec<TriggerBehavior>>,
    /// 分页偏移，默认 0
    #[param(source = "query")]
    pub offset: Option<usize>,
    /// 每页条数，默认 50、硬上限 200
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
    /// 新日程信息 / 备注（自由 JSON，Agent 自定义、无协议约束）；语义同创建时的 payload。
    /// 三态：字段缺省 → 保持原备注；显式 `null` → 清空备注；传 JSON → 覆盖。
    #[serde(
        default,
        deserialize_with = "crate::serde_ext::deserialize_tri_state",
        skip_serializing_if = "Option::is_none"
    )]
    pub payload: Option<Option<serde_json::Value>>,
    /// 新 cron 表达式（5 字段）
    pub cron_expression: Option<String>,
    /// 新间隔秒数
    pub interval_seconds: Option<i64>,
    /// 新一次性触发时刻（Unix 毫秒）
    pub run_at: Option<i64>,
    /// 新生效窗口起点（Unix 毫秒）；`null` 表示清除
    #[serde(
        default,
        deserialize_with = "crate::serde_ext::deserialize_tri_state",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_at: Option<Option<i64>>,
    /// 新生效窗口终点（Unix 毫秒）；`null` 表示清除
    #[serde(
        default,
        deserialize_with = "crate::serde_ext::deserialize_tri_state",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_at: Option<Option<i64>>,
    /// 新最大触发次数（必须为正数）；`null` 表示清除限制
    #[serde(
        default,
        deserialize_with = "crate::serde_ext::deserialize_tri_state",
        skip_serializing_if = "Option::is_none"
    )]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 更新 DTO 的四个三态字段必须能跨 JSON 边界：
    /// 缺省=保持、null=清空、值=覆盖。此前 `Option<Option<T>>` 的默认 serde
    /// 实现把 null 吞成外层 None，导致「清空」静默失效（既有测试直接构造
    /// Rust 结构体，绕过了 JSON，所以没发现）。
    #[test]
    fn update_schedule_tri_state_survives_json() {
        let base = r#"{"trigger_id":"t-1"}"#;
        let req: UpdateScheduleRequest = serde_json::from_str(base).unwrap();
        assert_eq!(req.payload, None);
        assert_eq!(req.start_at, None);
        assert_eq!(req.end_at, None);
        assert_eq!(req.max_runs, None);

        let clear = r#"{
            "trigger_id":"t-1",
            "payload":null,
            "start_at":null,
            "end_at":null,
            "max_runs":null
        }"#;
        let req: UpdateScheduleRequest = serde_json::from_str(clear).unwrap();
        assert_eq!(req.payload, Some(None));
        assert_eq!(req.start_at, Some(None));
        assert_eq!(req.end_at, Some(None));
        assert_eq!(req.max_runs, Some(None));

        let set = r#"{
            "trigger_id":"t-1",
            "payload":{"ids":["p-1"]},
            "start_at":100,
            "end_at":200,
            "max_runs":3
        }"#;
        let req: UpdateScheduleRequest = serde_json::from_str(set).unwrap();
        assert_eq!(req.payload, Some(Some(serde_json::json!({"ids":["p-1"]}))));
        assert_eq!(req.start_at, Some(Some(100)));
        assert_eq!(req.end_at, Some(Some(200)));
        assert_eq!(req.max_runs, Some(Some(3)));

        // 序列化方向：保持不出现、清空为 null、覆盖为值
        let serialized = serde_json::to_value(&req).unwrap();
        assert_eq!(serialized["payload"], serde_json::json!({"ids":["p-1"]}));
        let cleared: UpdateScheduleRequest = serde_json::from_str(clear).unwrap();
        let serialized = serde_json::to_value(&cleared).unwrap();
        assert!(serialized.get("payload").is_some_and(|v| v.is_null()));
        assert!(serialized.get("start_at").is_some_and(|v| v.is_null()));
    }

    /// 列表过滤参数必须能从 GET query 的逗号分隔字符串解析，默认由 handler 兜底为启用。
    #[test]
    fn list_request_status_and_behavior_lists_parse_from_query_string() {
        let req: ListCronTriggersRequest = serde_json::from_str(
            r#"{"statuses":"enabled,disabled","behaviors":"SyncMaintenance,AsyncNotify","offset":50,"limit":20}"#,
        )
        .unwrap();
        assert_eq!(
            req.statuses,
            Some(vec![TriggerStatus::Enabled, TriggerStatus::Disabled])
        );
        assert_eq!(
            req.behaviors,
            Some(vec![
                TriggerBehavior::SyncMaintenance,
                TriggerBehavior::AsyncNotify
            ])
        );
        assert_eq!(req.offset, Some(50));
        assert_eq!(req.limit, Some(20));

        let empty: ListCronTriggersRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.statuses, None);
        assert_eq!(empty.behaviors, None);
    }
}
