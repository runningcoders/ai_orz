//! Tool management API request/response DTOs - shared between backend and frontend

use crate::enums::{ControlMode, ToolProtocol, ToolStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 创建 Tool 请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateToolRequest {
    /// 工具名称（唯一）。
    pub name: String,
    /// 工具描述。
    pub description: String,
    /// 工具协议类型。
    pub protocol: ToolProtocol,
    /// 工具控制模式；为空时默认使用 Auto。
    pub control_mode: Option<ControlMode>,
    /// 协议配置（可能包含敏感信息，仅请求入参，响应不会原样返回）。
    pub config: Value,
    /// 参数 JSON Schema。
    pub parameters_schema: Option<Value>,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 初始状态；为空时默认使用 Enabled。
    pub status: Option<ToolStatus>,
}

/// Tool 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolListQuery {
    /// 关键词搜索。
    pub keyword: Option<String>,
    /// 只查询启用工具。
    pub enabled_only: Option<bool>,
    /// 按 Agent ID 查询已绑定工具。
    pub agent_id: Option<String>,
    /// 限制返回条数。
    pub limit: Option<usize>,
}

/// 更新 Tool 请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UpdateToolRequest {
    /// 工具名称。
    pub name: Option<String>,
    /// 工具描述。
    pub description: Option<String>,
    /// 工具协议类型。
    pub protocol: Option<ToolProtocol>,
    /// 工具控制模式。
    pub control_mode: Option<ControlMode>,
    /// 协议配置（可能包含敏感信息，仅请求入参，响应不会原样返回）。
    pub config: Option<Value>,
    /// 参数 JSON Schema。
    pub parameters_schema: Option<Value>,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
}

/// 更新 Tool 状态请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateToolStatusRequest {
    /// 目标状态。
    pub status: ToolStatus,
}

/// 绑定 Tool 到 Agent 请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BindToolToAgentRequest {
    /// Agent ID。
    pub agent_id: String,
}

/// 解绑 Tool 与 Agent 请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnbindToolFromAgentRequest {
    /// Agent ID。
    pub agent_id: String,
}

/// Tool 列表项响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolListItem {
    /// 工具 ID。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 工具描述。
    pub description: String,
    /// 工具协议类型。
    pub protocol: ToolProtocol,
    /// 工具控制模式。
    pub control_mode: ControlMode,
    /// 参数 JSON Schema。
    pub parameters_schema: Option<Value>,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 工具状态。
    pub status: ToolStatus,
    /// 是否存在协议配置；不返回配置原文，避免泄露敏感字段。
    pub has_config: bool,
    /// 创建时间戳。
    pub created_at: i64,
    /// 更新时间戳。
    pub updated_at: i64,
}

/// Tool 详情响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDetail {
    /// 工具 ID。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 工具描述。
    pub description: String,
    /// 工具协议类型。
    pub protocol: ToolProtocol,
    /// 工具控制模式。
    pub control_mode: ControlMode,
    /// 参数 JSON Schema。
    pub parameters_schema: Option<Value>,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 工具状态。
    pub status: ToolStatus,
    /// 是否存在协议配置；不返回配置原文，避免泄露敏感字段。
    pub has_config: bool,
    /// 创建人 ID。
    pub created_by: Option<String>,
    /// 最后修改人 ID。
    pub updated_by: Option<String>,
    /// 创建时间戳。
    pub created_at: i64,
    /// 更新时间戳。
    pub updated_at: i64,
}

/// Tool-Agent 绑定操作响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAgentBindingResponse {
    /// Agent ID。
    pub agent_id: String,
    /// Tool ID。
    pub tool_id: String,
}

/// 创建 Tool 响应。
pub type CreateToolResponse = ToolDetail;

/// 获取 Tool 响应。
pub type GetToolResponse = ToolDetail;

/// 更新 Tool 响应。
pub type UpdateToolResponse = ToolDetail;

/// 更新 Tool 状态响应。
pub type UpdateToolStatusResponse = ToolDetail;

/// 绑定 Tool 到 Agent 响应。
pub type BindToolToAgentResponse = ToolAgentBindingResponse;

/// 解绑 Tool 与 Agent 响应。
pub type UnbindToolFromAgentResponse = ToolAgentBindingResponse;
