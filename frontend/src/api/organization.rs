//! 组织管理 API

use common::api::{
    CreateLinkRequest, CreateLinkResponse, CreateOrganizationUserRequest,
    CreateOrganizationUserResponse, GetCurrentOrganizationResponse, GetCurrentUserResponse,
    IssuePairingCodeRequest, IssuePairingCodeResponse, ListLinksResponse,
    ListOrganizationsResponse, ListUsersResponse, UpdateCurrentOrganizationRequest,
    UpdateCurrentOrganizationResponse, UpdateCurrentUserRequest, UpdateCurrentUserResponse,
    UpdateUserRequest, UpdateUserResponse,
};

use super::{ApiError, api_delete, api_get, api_get_or_default, api_post, api_put};

/// 公开获取组织列表（无需登录，登录页用）
pub async fn list_organizations_public() -> Result<ListOrganizationsResponse, ApiError> {
    api_get("/api/v1/organization/list").await
}

/// 获取当前组织信息
pub async fn get_current_organization() -> Result<GetCurrentOrganizationResponse, ApiError> {
    api_get("/api/v1/organization/me").await
}

/// 更新当前组织信息
pub async fn update_current_organization(
    req: UpdateCurrentOrganizationRequest,
) -> Result<UpdateCurrentOrganizationResponse, ApiError> {
    api_put("/api/v1/organization/me", &req).await
}

/// 获取当前组织用户列表
pub async fn list_users() -> Result<ListUsersResponse, ApiError> {
    api_get_or_default("/api/v1/organization/user/me/list").await
}

/// 创建用户
pub async fn create_user(
    req: CreateOrganizationUserRequest,
) -> Result<CreateOrganizationUserResponse, ApiError> {
    api_post("/api/v1/organization/user/", &req).await
}

/// 更新用户
pub async fn update_user(req: UpdateUserRequest) -> Result<UpdateUserResponse, ApiError> {
    api_put("/api/v1/organization/user/update", &req).await
}

/// 删除用户
pub async fn delete_user(user_id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/organization/user/id/{}", user_id)).await
}

/// 获取当前用户信息
pub async fn get_current_user_info() -> Result<GetCurrentUserResponse, ApiError> {
    api_get("/api/v1/user/me").await
}

/// 更新当前用户信息
pub async fn update_current_user(
    req: UpdateCurrentUserRequest,
) -> Result<UpdateCurrentUserResponse, ApiError> {
    api_put("/api/v1/user/me", &req).await
}

// ===== 组织组网（关联组织，评审稿 §4.2 用户侧端点）=====

/// 签发组网配对码（本端管理员，10 分钟有效、单用途）
pub async fn issue_pairing_code() -> Result<IssuePairingCodeResponse, ApiError> {
    api_post(
        "/api/v1/organization/links/pairing/issue",
        &IssuePairingCodeRequest {},
    )
    .await
}

/// 发起建联（凭配对码 + 对端地址，服务端出站完成验证与凭证交换）
pub async fn create_link(req: CreateLinkRequest) -> Result<CreateLinkResponse, ApiError> {
    api_post("/api/v1/organization/links", &req).await
}

/// 已建联列表（前端"关联组织"页数据源）
pub async fn list_links() -> Result<ListLinksResponse, ApiError> {
    api_get("/api/v1/organization/links").await
}

/// 断联（本端管理员；连接置 Revoked，不删除记录）
pub async fn revoke_link(peer_org_id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/organization/links/{}", peer_org_id)).await
}
