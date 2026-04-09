//! Organization API client
//! All DTOs are imported from common crate shared with backend

use common::api::{
    CheckInitializedResponse,
    CreateUserRequest,
    GetCurrentOrganizationResponse,
    GetCurrentUserResponse,
    InitializeSystemRequest,
    ListOrganizationsResponse,
    ListUsersResponse,
    LoginRequest,
    LoginResponse,
    LogoutRequest,
    LogoutResponse,
    OrganizationInfoResponse,
    OrganizationListItem,
    UpdateCurrentUserRequest,
    UpdateCurrentOrganizationRequest,
    UserInfoResponse,
    UserListItem,
    EmptyResponse,
    ApiResponse,
};
use reqwest::Client;

/// Get backend API base URL
fn backend_url() -> &'static str {
    option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000")
}

/// Check if system has been initialized
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

    let api_resp: ApiResponse<CheckInitializedResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_else(|| CheckInitializedResponse { initialized: false }).initialized)
}

/// List all organizations (for login page selection)
pub async fn list_organizations() -> Result<Vec<OrganizationListItem>, String> {
    let url = format!("{}/api/v1/organization/list", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<ListOrganizationsResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_else(|| ListOrganizationsResponse { data: Vec::new(), total: 0 }).data)
}

/// Initialize system (create first organization and super admin)
pub async fn initialize_system(
    req: InitializeSystemRequest,
) -> Result<(), String> {
    let url = format!("{}/api/v1/organization/initialize", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&req).send().await {
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

/// User login
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

/// User logout
pub async fn logout() -> Result<LogoutResponse, String> {
    let url = format!("{}/api/v1/organization/auth/logout", backend_url());
    let client = Client::new();

    let response = match client.post(&url).json(&LogoutRequest {}).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<LogoutResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    api_resp.data.ok_or("响应为空".to_string())
}

/// Get current logged-in user information
pub async fn get_current_user_info() -> Result<UserInfoResponse, String> {
    let url = format!("{}/api/v1/user/me", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<GetCurrentUserResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.ok_or("响应为空".to_string())?.data)
}

/// Update current logged-in user information
pub async fn update_current_user_info(
    req: UpdateCurrentUserRequest,
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

    let api_resp: ApiResponse<EmptyResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(())
}

/// Get current user's organization information
pub async fn get_organization_info() -> Result<OrganizationInfoResponse, String> {
    let url = format!("{}/api/v1/organization/me", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<GetCurrentOrganizationResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.ok_or("响应为空".to_string())?.data)
}

/// Update current user's organization information
pub async fn update_organization_info(
    req: UpdateCurrentOrganizationRequest,
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

    let api_resp: ApiResponse<EmptyResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(())
}

/// List all users in current organization
pub async fn list_users_by_current_organization() -> Result<Vec<UserListItem>, String> {
    // organization_id is extracted from JWT by backend, no need to send from frontend
    let url = format!("{}/api/v1/organization/user/me/list", backend_url());
    let client = Client::new();

    let response = match client.get(&url).send().await {
        Ok(res) => res,
        Err(e) => return Err(e.to_string()),
    };

    if !response.status().is_success() {
        return Err(format!("HTTP 错误: {}", response.status()));
    }

    let api_resp: ApiResponse<ListUsersResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(api_resp.data.unwrap_or_else(|| ListUsersResponse { data: Vec::new(), total: 0 }).data)
}

/// Create new user in current organization
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

    let api_resp: ApiResponse<EmptyResponse> = match response.json().await {
        Ok(json) => json,
        Err(e) => return Err(e.to_string()),
    };

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }

    Ok(())
}
