//! Model Provider 管理 API 客户端

use common::api::{
    ModelProviderListItem,
    CreateModelProviderRequest,
    CreateModelProviderResponse,
    TestModelProviderConnectionResponse,
    EmptyResponse,
    ApiResponse,
};
use crate::config::current_config;
use reqwest::Client;

/// 获取 Model Provider 列表
pub async fn list_model_providers() -> Result<Vec<ModelProviderListItem>, String> {
    let config = current_config();
    let url = config.api_url("/api/v1/finance/model-providers");
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
    let config = current_config();
    let url = config.api_url("/api/v1/finance/model-providers");
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
    let config = current_config();
    let url = config.api_url(&format!("/api/v1/finance/model-providers/{id}"));
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
    let config = current_config();
    let url = config.api_url(&format!("/api/v1/finance/model-providers/{id}/test"));
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
