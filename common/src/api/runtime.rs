//! Runtime API 请求/响应 DTO
//!
//! Agent 运行时状态查询、取消思考、运行中 Agent 列表。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
