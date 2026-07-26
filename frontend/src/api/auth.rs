//! 认证相关 API

use common::api::{
    CheckInitializedResponse, InitializeSystemRequest, InitializeSystemResponse, LoginRequest,
    LoginResponse,
};

use super::{ApiError, api_get, api_post, api_post_empty};

pub async fn check_initialized() -> Result<CheckInitializedResponse, ApiError> {
    api_get("/api/v1/organization/initialize/check").await
}

pub async fn initialize_system(
    req: InitializeSystemRequest,
) -> Result<InitializeSystemResponse, ApiError> {
    api_post("/api/v1/organization/initialize", &req).await
}

pub async fn login(req: LoginRequest) -> Result<LoginResponse, ApiError> {
    api_post("/api/v1/organization/auth/login", &req).await
}

#[allow(dead_code)]
pub async fn logout() -> Result<(), ApiError> {
    // logout 不需要返回数据，但后端返回 ApiResponse<EmptyResponse>
    let req = serde_json::json!({});
    api_post_empty("/api/v1/organization/auth/logout", &req).await
}
