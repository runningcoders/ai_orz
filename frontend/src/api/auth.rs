//! 认证相关 API

use common::api::{
    CheckInitializedResponse, InitProgressResponse, InitializeSystemAsyncResponse,
    InitializeSystemRequest, LoginRequest, LoginResponse,
};

use super::{ApiError, api_get, api_post, api_post_empty};

pub async fn check_initialized() -> Result<CheckInitializedResponse, ApiError> {
    api_get("/api/v1/organization/initialize/check").await
}

/// 提交系统初始化（异步，返回 task_id）
pub async fn initialize_system(
    req: InitializeSystemRequest,
) -> Result<InitializeSystemAsyncResponse, ApiError> {
    api_post("/api/v1/organization/initialize", &req).await
}

/// 查询初始化进度（向后兼容接口，前端已改用统一 `get_task_progress`）
#[allow(dead_code)]
pub async fn get_initialize_progress(task_id: &str) -> Result<InitProgressResponse, ApiError> {
    api_get(&format!(
        "/api/v1/organization/initialize/progress?task_id={}",
        task_id
    ))
    .await
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
