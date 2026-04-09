//! Organization API 客户端

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// 系统初始化请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeSystemRequest {
    pub organization_name: String,
    pub description: Option<String>,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

/// 系统初始化响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeSystemResponse {
    pub organization_id: String,
    pub user_id: String,
}

/// 组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub status: i32,
}

/// API 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

impl<T> ApiResponse<T> {
    fn is_success(&self) -> bool {
        self.code == 0
    }

    fn data(self) -> Option<T> {
        self.data
    }
}

/// 获取后端 API 基础 URL
fn backend_url() -> &'static str {
    option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000")
}

/// 检查系统是否已初始化
pub async fn check_initialized() -> Result<bool, String> {
    let url = format!("{}/api/v1/organization/initialize/check", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<bool> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or(false))
}

/// 获取组织列表
pub async fn list_organizations() -> Result<Vec<OrganizationInfo>, String> {
    let url = format!("{}/api/v1/organization/list", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<Vec<OrganizationInfo>> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_default())
}

/// 初始化系统
pub async fn initialize_system(
    req: InitializeSystemRequest,
) -> Result<InitializeSystemResponse, String> {
    let url = format!("{}/api/v1/organization/initialize", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&req).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<InitializeSystemResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}
