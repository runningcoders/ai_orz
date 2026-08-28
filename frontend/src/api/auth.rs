//! 认证相关 API

use common::api::{
    CheckInitializedResponse, InitProgressResponse, InitializeSystemRequest,
    InviteCodeValidateResponse, LoginRequest, LoginResponse, RegisterByInviteRequest,
    TaskIdResponse,
};

use super::{ApiError, api_get, api_post, api_post_empty};

/// 检查系统是否已初始化
///
/// 协议化改造：后端不再返回裸 bool，改用标准 `CheckInitializedResponse` 结构体。
pub async fn check_initialized() -> Result<CheckInitializedResponse, ApiError> {
    api_get("/api/v1/organization/initialize/check").await
}

/// 提交系统初始化（异步，返回 task_id）
pub async fn initialize_system(req: InitializeSystemRequest) -> Result<TaskIdResponse, ApiError> {
    api_post("/api/v1/organization/initialize", &req).await
}

/// 查询初始化进度（公开接口，无需登录 —— 系统初始化发生在登录之前，
/// 不能调用需 JWT 的统一接口 `GET /system/tasks/{id}/progress`）
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

/// 邀请码注册（公开接口，不需要登录）
pub async fn register_by_invite(req: RegisterByInviteRequest) -> Result<LoginResponse, ApiError> {
    api_post("/api/v1/organization/auth/register", &req).await
}

/// RFC 3986 component 编码：unreserved 字符外全部百分号转义
fn percent_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// 校验邀请码有效性（公开接口，不需要登录）
pub async fn validate_invite_code(
    invite_code: &str,
) -> Result<InviteCodeValidateResponse, ApiError> {
    api_get(&format!(
        "/api/v1/organization/auth/invite/validate?invite_code={}",
        percent_encode_component(invite_code.trim())
    ))
    .await
}
