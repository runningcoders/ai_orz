//! Model Provider 管理 API 客户端

use common::api::{
    ModelProviderListItem,
    GetModelProviderResponse,
    CreateModelProviderRequest,
    CreateModelProviderResponse,
    UpdateModelProviderRequest,
    TestModelProviderConnectionResponse,
    EmptyResponse,
    ApiResponse,
};
use reqwest::Client;

/// 获取后端 API 基础 URL
fn backend_url() -> &'static str {
    option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000")
}

/// 获取 Model Provider 列表
pub async fn list_model_providers() -> Result<Vec<ModelProviderListItem>, String> {
    let url = format!("{}/api/v1/finance/model-providers", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<Vec<ModelProviderListItem>> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_default())
}

/// 创建新 Model Provider
pub async fn create_model_provider(req: CreateModelProviderRequest) -> Result<CreateModelProviderResponse, String> {
    let url = format!("{}/api/v1/finance/model-providers", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&req).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<CreateModelProviderResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// 删除 Model Provider
pub async fn delete_model_provider(id: &str) -> Result<(), String> {
    let url = format!("{}/api/v1/finance/model-providers/{id}", backend_url());
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

/// 测试 Model Provider 连通性
pub async fn test_model_provider_connection(id: &str) -> Result<TestModelProviderConnectionResponse, String> {
    let url = format!("{}/api/v1/finance/model-providers/{id}/test", backend_url());
    let client = Client::new();

    let response = match client.post(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    let api_resp: ApiResponse<TestModelProviderConnectionResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}
