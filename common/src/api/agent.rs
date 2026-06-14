//! Agent (AI智能体) related API request/response DTOs - shared between backend and frontend

use crate::enums::AgentStatus;
use serde::{Deserialize, Serialize};

/// 创建 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 获取 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 更新 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateAgentRequest {
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateAgentStatusRequest {
    /// 目标生命周期状态。
    ///
    /// 状态流转合法性由 HR Domain 校验；删除请优先使用 DELETE 接口。
    pub status: AgentStatus,
}

/// 更新 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAgentResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 更新 Agent 状态响应
pub type UpdateAgentStatusResponse = GetAgentResponse;
