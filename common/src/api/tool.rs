//! Built-in tool related API request/response DTOs - shared between backend and frontend

use crate::api::{PagedResult, PaginationParams};
use crate::enums::{ControlMode, ToolProtocol, ToolStatus};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create built-in tool request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateToolRequest {
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool protocol type
    pub protocol: ToolProtocol,
    /// Protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// Parameters JSON Schema (required for dynamic tools, optional for built-in tools)
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags list for capability matching and filtering
    pub tags: Option<Vec<String>>,
    /// Control mode: auto (native auto tool calling) / manual (custom pipeline)
    pub control_mode: Option<ControlMode>,
    /// Whether this tool is enabled
    pub enabled: Option<bool>,
}

/// Create built-in tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateToolResponse {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool type
    pub tool_type: String,
    /// Created timestamp
    pub created_at: i64,
}

/// Get tool request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（调用次数 + 失败次数）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
}

/// Get tool response (alias for tool detail)
pub type ToolDetail = GetToolResponse;

/// Get tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetToolResponse {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool protocol type
    pub protocol: ToolProtocol,
    /// Control mode: auto (native auto tool calling) / manual (custom pipeline)
    pub control_mode: ControlMode,
    /// Protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// Whether there is any non-empty configuration
    pub has_config: bool,
    /// 凭据需求声明（类型级：Builtin 工厂静态声明 / Mcp·Http 从 config 解析；非敏感直接展示）
    #[serde(default)]
    pub credential_requirements: Vec<crate::models::CredentialRequirement>,
    /// Parameters JSON Schema
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags list
    pub tags: Vec<String>,
    /// Whether this tool is enabled
    pub enabled: bool,
    /// Tool status
    pub status: ToolStatus,
    /// Created by user ID
    pub created_by: Option<String>,
    /// Updated by user ID
    pub updated_by: Option<String>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// 统计数据（调用次数 + 失败次数）
    pub stats: Option<crate::models::ToolStats>,
}

/// Delete tool request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteToolResponse {
    /// Whether deletion succeeded
    pub success: bool,
}

/// Debug call tool request (管理员调试用，跳过 Agent 授权)
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DebugCallToolRequest {
    /// Tool ID (from URL path)
    #[param(source = "path")]
    pub id: String,
    /// 工具调用参数 (JSON body)，需符合工具的 parameters_schema
    pub args: serde_json::Value,
}

/// Debug call tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugCallToolResponse {
    /// 调用是否成功
    pub success: bool,
    /// 工具业务返回值
    pub result: serde_json::Value,
    /// 工具调用 trace ID（可用于查询调用记录）
    pub tool_call_id: String,
    /// 执行状态文本
    pub status: String,
}

/// List tools request（语法糖：只接受分页参数，内部固定 created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListToolsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// List tools response
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListToolsResponse {
    /// List of all built-in tools
    pub tools: Vec<ToolListItem>,
}

/// Tool 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ToolQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 绑定的 Agent ID
    pub agent_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 协议类型
    pub protocol: Option<ToolProtocol>,
    /// 状态
    pub status: Option<ToolStatus>,
    /// MCP 服务器 ID
    pub mcp_server_id: Option<String>,
    /// 仅启用
    pub enabled_only: Option<bool>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Tool 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchToolsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索 + 向量语义搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 绑定的 Agent ID
    pub agent_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 协议类型
    pub protocol: Option<ToolProtocol>,
    /// 状态
    pub status: Option<ToolStatus>,
    /// MCP 服务器 ID
    pub mcp_server_id: Option<String>,
    /// 仅启用
    pub enabled_only: Option<bool>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Tool 响应（分页）
pub type SearchToolsResponse = PagedResult<ToolListItem>;

/// Tool list item alias (frontend compatibility)
pub type ListToolsResponseItem = ToolListItem;

/// 工具运行时就绪预检结果（三层就绪提示体系第①层：清单级标志）
///
/// 附加信息而非硬约束：未就绪不阻止绑定，仅作提示；
/// `unknown` 表示探测异常（best-effort，不阻塞列表接口）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RuntimeReady {
    /// 就绪（CLI 二进制可寻址 / 授权可用）
    Ready,
    /// 未就绪：含原因与可操作提示
    NotReady {
        /// 原因码：cli_not_installed / api_key_missing 等
        reason: String,
        /// 可操作提示（安装命令 / 配置路径）
        hint: String,
    },
    /// 探测异常，结果未知
    #[default]
    Unknown,
}

/// Tool list item
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ToolListItem {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: Option<String>,
    /// Tool protocol
    pub protocol: ToolProtocol,
    /// Control mode
    pub control_mode: ControlMode,
    /// Parameters JSON Schema (for MCP/HTTP tools)
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Tool status
    pub status: ToolStatus,
    /// Whether there is any non-empty configuration
    pub has_config: bool,
    /// Whether this tool is enabled
    pub enabled: bool,
    /// Creator ID
    pub created_by: String,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// Runtime readiness pre-check (CLI installed / authorization available; advisory only)
    pub runtime_ready: RuntimeReady,
}

/// Update tool request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,

    /// New tool display name
    pub name: Option<String>,
    /// New tool description
    pub description: Option<String>,
    /// New tool protocol type
    pub protocol: Option<ToolProtocol>,
    /// New control mode: auto (rig native) / manual (custom pipeline)
    pub control_mode: Option<ControlMode>,
    /// New protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// New parameters JSON Schema
    pub parameters_schema: Option<serde_json::Value>,
    /// New tags list
    pub tags: Option<Vec<String>>,
    /// New enabled status
    pub enabled: Option<bool>,
}

/// Update tool response
pub type UpdateToolResponse = GetToolResponse;

/// Update tool status request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateToolStatusRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
    /// New status
    pub status: ToolStatus,
}

/// Update tool status response
pub type UpdateToolStatusResponse = GetToolResponse;

/// Bind tool to agent request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct BindToolToAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// Tool ID to bind
    #[param(source = "path")]
    pub tool_id: String,
}

/// Bind tool to agent response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BindToolToAgentResponse {
    /// Whether binding succeeded
    pub success: bool,
}

/// Unbind tool from agent request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UnbindToolFromAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// Tool ID to unbind
    #[param(source = "path")]
    pub tool_id: String,
}

/// Unbind tool from agent response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UnbindToolFromAgentResponse {
    /// Whether unbinding succeeded
    pub success: bool,
}

/// Tool call status DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum ToolCallStatusDto {
    /// Tool invocation has started.
    Started,
    /// Tool invocation completed successfully.
    Completed,
    /// Tool invocation failed.
    Failed,
}

/// Query tool call trace entries.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryToolCallEntriesRequest {
    /// Exact call ID filter.
    #[param(source = "query")]
    pub call_id: Option<String>,
    /// Filter by Agent ID.
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// Filter by Project ID.
    #[param(source = "query")]
    pub project_id: Option<String>,
    /// Filter by Task ID.
    #[param(source = "query")]
    pub task_id: Option<String>,
    /// Filter by Tool ID.
    #[param(source = "query")]
    pub tool_id: Option<String>,
    /// Filter by call status.
    #[param(source = "query")]
    pub status: Option<ToolCallStatusDto>,
    /// Inclusive lower bound for started_at unix millis.
    #[param(source = "query")]
    pub started_after: Option<u64>,
    /// Inclusive upper bound for started_at unix millis.
    #[param(source = "query")]
    pub started_before: Option<u64>,
    /// Max result count. Defaults to 1 (latest matching entry).
    #[param(source = "query")]
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Query tool call trace entries response.
pub type QueryToolCallEntriesResponse = Vec<ToolCallEntryDetail>;

/// Get one tool call trace entry by call ID.
///
/// 该结构体用于 GET 路由（`/tool-call-entries/{call_id}`），因此除 path 字段外
/// 的所有字段**必须**显式标注 `#[param(source = "query")]`。
/// 未标注的字段会被 `generate_http_handler` 当成 body，从而生成 `axum::Json`
/// 提取器 —— GET 请求不带 `Content-Type: application/json` 时直接 415。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetToolCallEntryRequest {
    /// Exact call ID.
    #[param(source = "path")]
    pub call_id: String,
    /// Optional Tool ID narrows lookup to one tool trace directory.
    #[param(source = "query")]
    pub tool_id: Option<String>,
    /// Optional Agent ID access scope.
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// Optional Project ID access scope.
    #[param(source = "query")]
    pub project_id: Option<String>,
    /// Optional Task ID access scope.
    #[param(source = "query")]
    pub task_id: Option<String>,
}

/// Get one tool call trace entry response.
pub type GetToolCallEntryResponse = ToolCallEntryDetail;

/// List tool tags 请求（无参数，仅用于满足 handler 宏签名）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListToolTagsRequest {}

/// Tool tags 聚合响应（distinct tags from enabled tools）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListToolTagsResponse {
    /// 所有启用工具的不重复 tag 列表
    pub tags: Vec<String>,
}

/// Tool call trace entry detail.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ToolCallEntryDetail {
    /// Unique call ID.
    pub call_id: String,
    /// Tool ID.
    pub tool_id: String,
    /// Tool display name at call time.
    pub tool_name: String,
    /// Agent ID that initiated this call, if available.
    pub agent_id: Option<String>,
    /// Task ID associated with this call, if available.
    pub task_id: Option<String>,
    /// Project ID associated with this call, if available.
    pub project_id: Option<String>,
    /// Start timestamp in unix milliseconds.
    pub started_at: u64,
    /// Finish timestamp in unix milliseconds.
    pub finished_at: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Redacted input arguments captured in trace storage.
    pub input: serde_json::Value,
    /// Redacted output result captured in trace storage.
    pub output: Option<serde_json::Value>,
    /// Redacted error marker captured in trace storage.
    pub error: Option<String>,
    /// Call status.
    pub status: ToolCallStatusDto,
    /// Redacted additional trace metadata.
    pub metadata: serde_json::Value,
}

/// GET /api/v1/finance/tools/runtime-stats 请求
///
/// 工作台顶栏运行时读数用：查询当前组织最近 N 分钟的工具调用汇总。
/// 与 `/finance/model-providers/token-stats` 同属「组织级统计读数」一族，
/// 区别是本接口只给合计值、不返回时序点。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Params)]
pub struct GetToolRuntimeStatsRequest {
    /// 时间窗口（分钟），默认 60，上限 1440（24 小时）
    #[param(source = "query")]
    pub minutes: Option<u32>,
}

/// GET /api/v1/finance/tools/runtime-stats 响应
///
/// 所有字段均为「窗口内合计」。窗口内无调用时：
/// `total_calls` / `failed_calls` 为 0，`avg_duration_ms` 为 None（不是 0，避免把
/// "没有调用" 误读成 "耗时 0ms"）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ToolRuntimeStatsResponse {
    /// 实际生效的时间窗口（分钟），已按 [1, 1440] clamp
    pub window_minutes: u32,
    /// 工具调用总次数
    pub total_calls: u64,
    /// 工具调用失败次数（`status = failed`）
    pub failed_calls: u64,
    /// 平均调用耗时（毫秒），窗口内无调用时为 None
    pub avg_duration_ms: Option<f64>,
}

// ==================== 工具授权（阶段③批次一 · S1b DTO 契约）====================

/// 授权单六态（领域镜像：pkg::authorization::AuthorizationStatus）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuthorizationStatusDto {
    /// 待审批
    Pending,
    /// 已批准生效
    Active,
    /// 已过期
    Expired,
    /// 已撤销
    Revoked,
    /// 已拒绝
    Rejected,
    /// 已消耗
    Consumed,
}

/// 证据类别：UI 直批 / 聊天指令直批（无 Agent 参与）/ 聊天确认代呈（Agent 携证据）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EvidenceClassDto {
    /// 平台界面直批（user ctx 即身份凭证）
    Ui,
    /// 聊天指令直批（渠道入站管线以消息归属人身份直调，无 Agent 参与）
    ChatDirective,
    /// 聊天确认代呈（Agent 携证据消息代呈，红线⑤运行时强制）
    ChatMediated,
}

/// 授权范围（Approve 可裁量；默认精确签名最窄授权）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthorizationScopeDto {
    /// 授权命令签名（None=沿用建单签名）
    pub command_signature: Option<String>,
    /// true=前缀匹配放宽（默认 false 精确匹配）
    pub prefix_match: bool,
    /// 次数上限（None=按规则幂等属性默认：非幂等 1 次/幂等不限）
    pub max_uses: Option<u32>,
    /// 有效期秒（None=默认 900，clamp [60,3600]）
    pub ttl_secs: Option<i64>,
}

/// 创建授权请求（主动建单：口头事前授权登记等场景）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateAuthorizationRequest {
    /// 申请人 Agent ID
    pub agent_id: String,
    /// 目标工具 ID
    pub tool_id: String,
    /// 申请理由（审计留痕）
    pub reason: String,
    /// 受限命令原文（规范化后即签名；主动建单必携——纵深校验与签名计算依据）
    pub command: Option<String>,
    /// 拓展预留：仓库标识
    pub repository: Option<String>,
    /// 拓展预留：分支
    pub branch: Option<String>,
    /// 拓展预留：组织
    pub org_id: Option<String>,
}

/// 创建授权响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateAuthorizationResponse {
    /// 授权单 ID
    pub authorization_id: String,
    /// 当前状态
    pub status: AuthorizationStatusDto,
}

/// 审批决策请求（UI 直批通道；Agent 不得调用，运行时强制 user ctx）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AuthorizationDecisionRequest {
    /// 授权单 ID
    pub authorization_id: String,
    /// 决策：Approve / Reject
    pub decision: String,
    /// 授权范围裁量
    pub scope: Option<AuthorizationScopeDto>,
    /// 证据类别（UI 直批可省略；聊天指令直批必携证据消息）
    pub evidence_class: Option<EvidenceClassDto>,
    /// 证据消息 ID（证据链核心，平台校验五要素）
    pub evidence_message_id: Option<String>,
    /// 代呈 Agent ID（仅代呈通道；由 handler 从 ctx 注入，DTO 预留向后兼容）
    pub mediator_agent_id: Option<String>,
}

/// 审批决策响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthorizationDecisionResponse {
    /// 授权单 ID
    pub authorization_id: String,
    /// 决策后状态
    pub status: AuthorizationStatusDto,
    /// Approve 签发的授权 ID
    pub grant_id: Option<String>,
}

/// 授权单查询请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AuthorizationQueryRequest {
    /// 按状态过滤
    pub status: Option<AuthorizationStatusDto>,
    /// 按申请人 Agent 过滤
    pub agent_id: Option<String>,
    /// 按工具过滤
    pub tool_id: Option<String>,
    /// 按归属用户过滤
    pub user_id: Option<String>,
}

/// 授权单详情
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthorizationDetailDto {
    /// 授权单 ID
    pub authorization_id: String,
    /// 申请人 Agent
    pub agent_id: String,
    /// 目标工具
    pub tool_id: String,
    /// 归属用户（审批人）
    pub user_id: String,
    /// 受限命令规范化签名
    pub command_signature: String,
    /// 命中拦截规则 id
    pub blocking_rule: String,
    /// 建单时刻 ms
    pub requested_at_ms: i64,
    /// 触发本次拦截建单的工具调用 ID（= 工具调用记录 `call_id`）
    ///
    /// 前端据此把授权单挂到对应调用记录的详情/行内快捷审批上；
    /// 主动建单（无被拦调用上下文）为 null。
    pub call_id: Option<String>,
    /// 当前状态
    pub status: AuthorizationStatusDto,
    /// 已签发授权 ID（Pending/Rejected 为 None）
    pub grant_id: Option<String>,
    /// 过期时刻 ms
    pub expires_at_ms: Option<i64>,
    /// 剩余可用次数（None=不限）
    pub remaining_uses: Option<u32>,
}

/// 撤销授权请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RevokeAuthorizationRequest {
    /// 授权单 ID
    pub authorization_id: String,
    /// 撤销理由（审计留痕）
    pub reason: Option<String>,
}

/// 撤销授权响应（与决策响应同构）
pub type RevokeAuthorizationResponse = AuthorizationDecisionResponse;
