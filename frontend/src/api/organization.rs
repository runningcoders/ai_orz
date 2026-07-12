//! 组织管理 API

use common::api::{
    CreateOrganizationUserRequest, CreateOrganizationUserResponse, GetCurrentOrganizationResponse,
    GetCurrentUserResponse, ListOrganizationsResponse, ListUsersResponse,
    UpdateCurrentOrganizationRequest, UpdateCurrentOrganizationResponse, UpdateCurrentUserRequest,
    UpdateCurrentUserResponse, UpdateUserRequest, UpdateUserResponse,
};

use super::{api_get, api_get_or_default, api_post, api_put};

/// 公开获取组织列表（无需登录，登录页用）
pub async fn list_organizations_public() -> Result<ListOrganizationsResponse, String> {
    api_get("/api/v1/organization/list").await
}

/// 获取当前组织信息
pub async fn get_current_organization() -> Result<GetCurrentOrganizationResponse, String> {
    api_get("/api/v1/organization/me").await
}

/// 更新当前组织信息
pub async fn update_current_organization(req: UpdateCurrentOrganizationRequest) -> Result<UpdateCurrentOrganizationResponse, String> {
    api_put("/api/v1/organization/me", &req).await
}

/// 获取当前组织用户列表
pub async fn list_users() -> Result<ListUsersResponse, String> {
    api_get_or_default("/api/v1/organization/user/me/list").await
}

/// 创建用户
pub async fn create_user(req: CreateOrganizationUserRequest) -> Result<CreateOrganizationUserResponse, String> {
    api_post("/api/v1/organization/user/", &req).await
}

/// 更新用户
pub async fn update_user(req: UpdateUserRequest) -> Result<UpdateUserResponse, String> {
    api_put("/api/v1/organization/user/update", &req).await
}

/// 删除用户
pub async fn delete_user(user_id: &str) -> Result<(), String> {
    super::api_delete(&format!("/api/v1/organization/user/id/{}", user_id)).await
}

/// 获取当前用户信息
pub async fn get_current_user_info() -> Result<GetCurrentUserResponse, String> {
    api_get("/api/v1/user/me").await
}

/// 更新当前用户信息
pub async fn update_current_user(req: UpdateCurrentUserRequest) -> Result<UpdateCurrentUserResponse, String> {
    api_put("/api/v1/user/me", &req).await
}
