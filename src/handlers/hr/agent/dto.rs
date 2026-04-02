//! Agent Handler 请求/响应 DTO

use crate::models::agent::Agent;
use serde::{Deserialize, Serialize};

// ==================== 请求结构体 ====================

/// 创建 Agent 请求
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub soul: String,
    pub model_provider_id: String,
}

/// 更新 Agent 请求
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub role: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
}

// ==================== 响应结构体 ====================

/// Agent 响应
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub soul: String,
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl AgentResponse {
    /// 从 Agent 转换为响应
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            id: agent.po.id.clone(),
            name: agent.po.name.clone(),
            role: agent.po.role.clone(),
            capabilities: agent.po.get_capabilities(),
            soul: agent.po.soul.clone(),
            created_by: agent.po.created_by.clone(),
            modified_by: agent.po.modified_by.clone(),
            created_at: agent.po.created_at,
            updated_at: agent.po.updated_at,
        }
    }
}
