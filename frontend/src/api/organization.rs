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
    pub created_at: i64,
}

/// 登录请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub organization_id: String,
    pub username: String,
    pub password_hash: String,
}

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user_id: String,
    pub organization_id: String,
    pub username: String,
}

/// 用户列表项（用于用户管理列表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListItem {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: i32,
    pub status: i32,
    pub created_at: i64,
}

/// 创建用户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub password_hash: String,
    pub role: i32,
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

/// 用户登录
pub async fn login(
    req: LoginRequest,
) -> Result<LoginResponse, String> {
    let url = format!("{}/api/v1/organization/auth/login", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&req).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<LoginResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// 当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub organization_id: String,
    pub role: i32,
    pub role_name: String,
    pub status: i32,
}

/// 更新当前用户信息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub password_hash: Option<String>,
}

/// 获取当前登录用户信息
pub async fn get_current_user_info() -> Result<UserInfo, String> {
    let url = format!("{}/api/v1/user/me", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<UserInfo> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// 更新当前登录用户信息
pub async fn update_current_user_info(
    req: UpdateUserRequest,
) -> Result<(), String> {
    let url = format!("{}/api/v1/user/me", backend_url());
    let client = Client::new();

    let response = match client.put(&url).json(&req).send().await {
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

/// 更新组织信息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrganizationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub base_url: Option<String>,
}

/// 获取当前用户所在组织信息
pub async fn get_organization_info() -> Result<OrganizationInfo, String> {
    let url = format!("{}/api/v1/organization/me", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<OrganizationInfo> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// 更新当前用户所在组织信息
pub async fn update_organization_info(
    req: UpdateOrganizationRequest,
) -> Result<(), String> {
    let url = format!("{}/api/v1/organization/me", backend_url());
    let client = Client::new();

    let response = match client.put(&url).json(&req).send().await {
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

/// 获取当前组织下的所有用户列表
pub async fn list_users_by_current_organization() -> Result<Vec<UserListItem>, String> {
    // organization_id 从 RequestContext 的 organization_id 获取，后端直接从 token 提取
    // 直接使用正确的 URL，后端会从 token 获取 organization_id
    let url = format!("{}/api/v1/organization/user/me/list", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<Vec<UserListItem>> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_default())
}

/// 在当前组织下创建新用户
pub async fn create_user(
    req: CreateUserRequest,
) -> Result<(), String> {
    let url = format!("{}/api/v1/organization/user/", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&req).send().await {
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
