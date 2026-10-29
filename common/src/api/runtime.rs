//! Runtime API 请求/响应 DTO
//!
//! Agent 运行时状态查询、取消思考、运行中 Agent 列表。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{PagedResult, PaginationParams};
use crate::enums::ThinkingIntensity;

/// GET /agents/{id}/runtime-status 请求（path 参数：id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RuntimeStatusRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// GET /agents/{id}/runtime-status 响应
///
/// 包含 Agent 运行时状态 + 思考运行时快照（如有）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeStatusResponse {
    /// Agent ID
    pub agent_id: String,
    /// 运行时状态："idle" / "busy" / "resting"
    pub state: String,
    /// 当前处理的消息 ID（仅 Busy 时有值）
    pub current_message_id: Option<String>,
    /// 当前关联的任务 ID
    pub task_id: Option<String>,
    /// 当前关联的项目 ID
    pub project_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
    /// 思考运行时快照（仅 Busy 时有值）
    pub think_runtime: Option<ThinkRuntimeInfo>,
}

/// 思考运行时信息（前端展示用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkRuntimeInfo {
    /// 当前 trace_id（日志检索用）
    pub trace_id: String,
    /// 场景："awaken" / "settle" / "summary" / "intent-analyze"
    pub scene: String,
    /// 当前轮次
    pub round: usize,
    /// 最大轮次
    pub max_rounds: usize,
    /// 累计输入 token
    pub tokens_input: u64,
    /// 累计输出 token
    pub tokens_output: u64,
    /// 累计总 token
    pub total_tokens: u64,
    /// 工具调用次数
    pub tool_call_count: usize,
    /// 思考状态："thinking" / "cancelled" / "finished"
    pub status: String,
    /// 思考开始时间戳（毫秒）
    pub started_at: i64,
    /// 最后更新时间戳（毫秒）
    pub last_updated_at: i64,
}

/// POST /agents/{id}/cancel-thinking 请求（path 参数：id，无 body）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CancelThinkingRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// POST /agents/{id}/cancel-thinking 响应
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancelThinkingResponse {
    /// 是否成功取消（false 表示 Agent 当前未在思考）
    pub success: bool,
    /// 描述信息
    pub message: String,
}

/// GET /agents/runtime-list 请求参数（全部 query 参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RuntimeListRequest {
    /// 按状态过滤："busy" / "resting" / "idle"（不传则返回全部）
    #[param(source = "query")]
    #[serde(default)]
    pub state: Option<String>,
    /// 按任务 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按项目 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub project_id: Option<String>,
}

/// GET /agents/runtime-list 响应
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeListResponse {
    /// 运行中 Agent 列表
    pub items: Vec<RuntimeStatusResponse>,
    /// 总数
    pub total: usize,
}

/// GET /agents/think-traces 请求
///
/// `agent_id` 省略时跨 Agent 聚合（对话页多 Agent 思考流）；
/// `project_id` / `task_id` 按轨迹条目字段过滤。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryThinkTracesRequest {
    /// 按 Agent 过滤；省略则遍历全部 Agent
    #[param(source = "query")]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// 按项目过滤（仅 2026-10-22 之后写入的轨迹带该字段）
    #[param(source = "query")]
    #[serde(default)]
    pub project_id: Option<String>,
    /// 按任务过滤（仅 2026-10-22 之后写入的轨迹带该字段）
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按 run 精确查询（awaken / settle / compact 下即 MemoryTrace.id）
    #[param(source = "query")]
    #[serde(default)]
    pub run_id: Option<String>,
    /// 思考场景（awaken / settle / summary / compact / intent-analyze）
    #[param(source = "query")]
    #[serde(default)]
    pub scene: Option<String>,
    /// created_at 下界（毫秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub started_after: Option<u64>,
    /// created_at 上界（毫秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub started_before: Option<u64>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 思考轨迹分页响应
pub type QueryThinkTracesResponse = PagedResult<ThinkTraceListItem>;

/// 思考轨迹条目（含节点 occurrence 与选路记录，展开即用，不再二次请求）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceListItem {
    /// 本次 run 的唯一 ID（awaken / settle / compact 下同时是 MemoryTrace.id）
    pub run_id: String,
    /// 发起思考的 Agent
    pub agent_id: String,
    /// 思考场景
    pub scene: String,
    /// 轨迹蓝图 ID
    pub path: String,
    /// 图执行步数
    pub steps: usize,
    /// advisor 介入次数
    pub advisor_calls: usize,
    /// 轨迹指纹
    pub fingerprint: String,
    /// 落盘时间戳（毫秒）
    pub created_at: u64,
    /// 关联项目 ID
    pub project_id: Option<String>,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 轮级明细（思考文本 / 推理摘要 / 工具调用 / 裁决）
    pub rounds: Vec<ThinkTraceRoundDto>,
    /// 节点 occurrence 序列
    pub occurrences: Vec<ThinkTraceOccurrenceDto>,
    /// 选路记录
    pub routes: Vec<ThinkTraceRouteDto>,
}

/// 单轮思考明细
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceRoundDto {
    /// 轮次编号（1-based）
    pub round: usize,
    /// 本轮模型可见文本
    pub assistant_text: Option<String>,
    /// 本轮推理摘要（仅 provider 返回的可读 summary）
    pub reasoning: Vec<String>,
    /// 本轮工具调用
    pub tool_calls: Vec<ThinkTraceToolCallDto>,
    /// 本轮输入 token
    pub input_tokens: u64,
    /// 本轮输出 token
    pub output_tokens: u64,
    /// 本轮 LLM 调用耗时（毫秒）
    pub duration_ms: u64,
    /// 裁决结果：final / continue / reflect / exit
    pub decision: Option<String>,
    /// 本轮调用模型时实际下发的思考强度（0–100 分）；None = Provider 未启用该能力
    #[serde(default)]
    pub thinking_intensity: Option<ThinkingIntensity>,
}

/// 单轮内的一次工具调用
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceToolCallDto {
    /// 工具调用 trace ID（可据此查 tool_tracing 的完整入参/出参）
    pub call_id: Option<String>,
    /// 工具 wire name
    pub name: String,
    /// 入参预览（截断）
    pub arguments: String,
    /// 结果预览（截断）
    pub result: Option<String>,
    /// 执行状态：ok / error / blocked / not_found / pending
    pub status: String,
}

/// 单节点执行记录
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceOccurrenceDto {
    /// 节点名
    pub node: String,
    /// 同节点执行序号
    pub seq: usize,
    /// 图步编号
    pub step: usize,
    /// 节点产出信号
    pub signal: Option<String>,
}

/// 单次选路记录
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceRouteDto {
    /// 图步编号
    pub step: usize,
    /// 候选节点
    pub candidates: Vec<String>,
    /// 实际选择节点
    pub chosen: String,
    /// 选路来源（unique / deterministic / advisor / fallback）
    pub via: String,
    /// 选择原因
    pub reason: Option<String>,
}

// ==================== 记忆轨迹 ====================

/// GET /agents/memory-traces 请求
///
/// `agent_id` 省略时跨 Agent 聚合（Admin 全量视图）；时间过滤单位为**秒**
/// （与 `MemoryTrace.created_at` 一致），与思考轨迹的毫秒口径刻意区分。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryMemoryTracesRequest {
    /// 按 Agent 过滤；省略则遍历全部 Agent
    #[param(source = "query")]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// 按任务过滤
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按请求日志 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub log_id: Option<String>,
    /// 按 trace ID 精确查询（详情弹窗之外的列表定位）
    #[param(source = "query")]
    #[serde(default)]
    pub trace_id: Option<String>,
    /// created_at 下界（秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub created_after: Option<i64>,
    /// created_at 上界（秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub created_before: Option<i64>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 记忆轨迹分页响应
pub type QueryMemoryTracesResponse = PagedResult<MemoryTraceListItem>;

/// 记忆轨迹列表项（input/output 仅返回预览，完整内容走详情接口）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryTraceListItem {
    /// 轨迹 ID（awaken / settle / compact 下与 think trace 的 run_id 相同）
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 请求日志 ID
    pub log_id: String,
    /// 创建者用户 ID
    pub user_id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色（system / user / assistant / summary）
    pub role: String,
    /// 创建时间（秒）
    pub created_at: i64,
    /// 完成时间（秒）
    pub completed_at: Option<i64>,
    /// 输入预览（截断）
    pub input_preview: String,
    /// 输出预览（截断；中断时为空）
    pub output_preview: Option<String>,
    /// 元数据
    pub metadata: std::collections::HashMap<String, String>,
}

/// GET /agents/memory-traces/{trace_id} 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetMemoryTraceRequest {
    /// 轨迹 ID
    #[param(source = "path")]
    pub trace_id: String,
    /// 轨迹所属 Agent（定位 `agents/{agent_id}/memory/` 目录）
    #[param(source = "query")]
    pub agent_id: String,
}

/// 记忆轨迹详情（完整输入输出）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryTraceDetail {
    /// 轨迹 ID
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 请求日志 ID
    pub log_id: String,
    /// 创建者用户 ID
    pub user_id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色（system / user / assistant / summary）
    pub role: String,
    /// 创建时间（秒）
    pub created_at: i64,
    /// 完成时间（秒）
    pub completed_at: Option<i64>,
    /// 完整输入（Prompt）
    pub input: String,
    /// 完整输出（中断时为空）
    pub output: Option<String>,
    /// 元数据
    pub metadata: std::collections::HashMap<String, String>,
}
