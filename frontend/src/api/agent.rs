//! Agent 管理 API 客户端

use serde::{Deserialize, Serialize};

/// Agent 列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListItem {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    pub model_provider_id: String,
    pub created_at: i64,
}

/// 获取 Agent 详情响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResponse {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
    pub model_provider_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 创建 Agent 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub role: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
    pub model_provider_id: String,
}

/// 创建 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentResponse {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    pub created_at: i64,
}

/// 更新 Agent 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub role: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
}

/// API 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    pub fn data(self) -> Option<T> {
        self.data
    }
}

/// 获取后端 API 基础 URL
fn backend_url() -> &'static str {
    option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000")
}

/// 获取 Agent 列表
pub async fn list_agents() -> Result<Vec<AgentListItem>, String> {
    let url = format!("{}/api/v1/hr/agents", backend_url());
    let client = reqwest::Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<Vec<AgentListItem>> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_default())
}

/// 创建新 Agent
pub async fn create_agent(req: CreateAgentRequest) -> Result<CreateAgentResponse, String> {
    let url = format!("{}/api/v1/hr/agents", backend_url());
    let client = reqwest::Client::new();

    let response = match client.post(&url).json(&req).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<CreateAgentResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// 删除 Agent
pub async fn delete_agent(id: &str) -> Result<(), String> {
    let url = format!("{}/api/v1/hr/agents/{}", backend_url(), id);
    let client = reqwest::Client::new();

    let response = match client.delete(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<()> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(())
}
