//! 认证相关 API

use common::api::{
    CheckInitializedResponse, InitializeSystemRequest, InitializeSystemResponse, LoginRequest,
    LoginResponse,
};

use super::{api_get, api_post};

pub async fn check_initialized() -> Result<CheckInitializedResponse, String> {
    api_get("/api/v1/organization/initialize/check").await
}

pub async fn initialize_system(req: InitializeSystemRequest) -> Result<InitializeSystemResponse, String> {
    api_post("/api/v1/organization/initialize", &req).await
}

pub async fn login(req: LoginRequest) -> Result<LoginResponse, String> {
    api_post("/api/v1/organization/auth/login", &req).await
}

pub async fn logout() -> Result<(), String> {
    // logout 不需要返回数据，但后端返回 ApiResponse<EmptyResponse>
    let req = serde_json::json!({});
    super::api_post_empty("/api/v1/organization/auth/logout", &req).await
}
