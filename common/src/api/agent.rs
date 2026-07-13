//! Agent (AI智能体) related API request/response DTOs - shared between backend and frontend

use crate::enums::{AgentRuntimeState, AgentStatus};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 创建 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateAgentRequest {
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Option<Vec<String>>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: String,
}

/// 创建 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
}

/// Agent 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentListItem {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Vec<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: String,
    /// 生命周期状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
    /// 运行时状态（内存状态）
    pub runtime_state: i32,
}

/// 获取 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// 获取 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Vec<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: String,
    /// 生命周期状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
    /// 运行时状态（内存状态，服务重启后重置）
    pub runtime_state: i32,
    /// 当前处理的消息 ID（仅忙碌时有效）
    pub current_message_id: Option<String>,
    /// 已绑定的工具 ID 列表
    pub tools: Vec<String>,
}

/// 更新 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,

    /// Agent 名称
    pub name: Option<String>,
    /// Agent 角色标签列表
    pub roles: Option<Vec<String>>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: Option<String>,
}

/// 更新 Agent 状态请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateAgentStatusRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,

    /// 目标生命周期状态。
    ///
    /// 状态流转合法性由 HR Domain 校验；删除请优先使用 DELETE 接口。
    pub status: AgentStatus,
}

/// 删除 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// 更新 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: String,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 删除 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteAgentResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 获取 Agent 列表请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentsRequest {
    /// 可选状态筛选
    #[param(source = "query")]
    pub status: Option<AgentStatus>,
}

/// 获取 Agent 列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListAgentsResponse {
    /// Agent 列表
    pub agents: Vec<AgentListItem>,
}

/// Agent 列表项响应别名（前端兼容）
pub type ListAgentsResponseItem = AgentListItem;

/// 搜索 Agent 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchAgentsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索）
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 返回结果数量限制
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// 搜索 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchAgentsResponse {
    /// Agent 列表
    pub agents: Vec<AgentListItem>,
}

/// 更新 Agent 状态响应
pub type UpdateAgentStatusResponse = GetAgentResponse;

/// 安装工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InstallToolPackRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,

    /// 工具包 tag（如 "project_management"）
    #[param(source = "path")]
    pub tag: String,
}

/// 安装工具包响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallToolPackResponse {
    /// Agent ID
    pub agent_id: String,
    /// 已安装的工具包 tags
    pub installed_tags: Vec<String>,
}

/// 卸载工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallToolPackRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,

    /// 工具包 tag
    #[param(source = "path")]
    pub tag: String,
}

/// 卸载工具包响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallToolPackResponse {
    /// Agent ID
    pub agent_id: String,
    /// 剩余已安装的工具包 tags
    pub installed_tags: Vec<String>,
}

/// 列出已安装工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListInstalledToolPacksRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出已安装工具包响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListInstalledToolPacksResponse {
    /// Agent ID
    pub agent_id: String,
    /// 已安装的工具包 tags
    pub installed_tags: Vec<String>,
}
