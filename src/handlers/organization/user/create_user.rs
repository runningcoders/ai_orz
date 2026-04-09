//! 创建新用户接口

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::constants::request_context::RequestContext;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::pkg::constants::UserRole;
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 创建新用户请求（在当前组织下创建）
/// organization_id 从 RequestContext 获取，不需要前端传递
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    /// 用户名
    pub username: String,
    /// 密码哈希
    pub password_hash: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 用户角色编号（1=Member, 2=Admin, 3=SuperAdmin）
    pub role: i32,
}

/// 创建新用户响应
#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    /// 用户 ID
    pub user_id: String,
}

/// 创建新用户
/// 在当前登录用户所在组织下创建新用户，organization_id 从 RequestContext 获取
pub async fn create_user(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 从 RequestContext 获取当前组织 ID
    let organization_id = ctx.organization_id.clone().ok_or_else(|| {
        AppError::BadRequest("未找到组织信息".to_string())
    })?;

    let domain = organization::domain();

    // 生成随机用户 ID
    let user_id = generate_id();

    // 转换角色
    let role = match req.role {
        2 => UserRole::Admin,
        3 => UserRole::SuperAdmin,
        _ => UserRole::Member,
    };

    // 创建 UserPo
    let user = UserPo::new(
        user_id.clone(),
        organization_id.clone(),
        req.username.clone(),
        req.display_name.clone().unwrap_or_default(),
        req.email.clone().unwrap_or_default(),
        req.password_hash.clone(),
        role,
        ctx.user_id.clone().unwrap_or_default(),
    );

    domain.user_manage().create_user(ctx, user)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(CreateUserResponse {
        user_id,
    }))).into_response())
}

/// 生成随机 ID
fn generate_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const ID_LEN: usize = 16;
    let mut rng = rand::thread_rng();
    (0..ID_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
