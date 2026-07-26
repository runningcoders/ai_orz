//! Built-in tool related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
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
    /// Control mode: auto (rig native) / manual (custom pipeline)
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
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
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
    /// Control mode: auto (rig native) / manual (custom pipeline)
    pub control_mode: ControlMode,
    /// Protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// Whether there is any non-empty configuration
    pub has_config: bool,
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

/// Tool list item alias (frontend compatibility)
pub type ListToolsResponseItem = ToolListItem;

/// Tool list item
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    pub call_id: Option<String>,
    /// Filter by Agent ID.
    pub agent_id: Option<String>,
    /// Filter by Project ID.
    pub project_id: Option<String>,
    /// Filter by Task ID.
    pub task_id: Option<String>,
    /// Filter by Tool ID.
    pub tool_id: Option<String>,
    /// Filter by call status.
    pub status: Option<ToolCallStatusDto>,
    /// Inclusive lower bound for started_at unix millis.
    pub started_after: Option<u64>,
    /// Inclusive upper bound for started_at unix millis.
    pub started_before: Option<u64>,
    /// Max result count. Defaults to 1 (latest matching entry).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Query tool call trace entries response.
pub type QueryToolCallEntriesResponse = Vec<ToolCallEntryDetail>;

/// Get one tool call trace entry by call ID.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetToolCallEntryRequest {
    /// Exact call ID.
    #[param(source = "path")]
    pub call_id: String,
    /// Optional Tool ID narrows lookup to one tool trace directory.
    pub tool_id: Option<String>,
    /// Optional Agent ID access scope.
    pub agent_id: Option<String>,
    /// Optional Project ID access scope.
    pub project_id: Option<String>,
    /// Optional Task ID access scope.
    pub task_id: Option<String>,
}

/// Get one tool call trace entry response.
pub type GetToolCallEntryResponse = ToolCallEntryDetail;

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
