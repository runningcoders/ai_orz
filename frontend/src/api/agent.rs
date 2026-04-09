//! Agent 管理 API 客户端

use common::api::{
    AgentListItem,
    CreateAgentRequest,
    CreateAgentResponse,
    EmptyResponse,
    ApiResponse,
};
use crate::config::current_config;
use reqwest::Client;

/// 获取 Agent 列表
pub async fn list_agents() -> Result<Vec<AgentListItem>, String> {
    let config = current_config();
    let url = config.api_url("/api/v1/hr/agents");
    let client = Client::new();

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
    let config = current_config();
    let url = config.api_url("/api/v1/hr/agents");
    let client = Client::new();

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
    let config = current_config();
    let url = config.api_url(&format!("/api/v1/hr/agents/{}", id));
    let client = Client::new();

    let response = match client.delete(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<EmptyResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(())
}
