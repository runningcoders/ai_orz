//! 创建新用户接口

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use axum::{
    extract::{Json},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::pkg::constants::UserRole;
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 创建新用户请求
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    /// 组织 ID
    pub organization_id: String,
    /// 用户名
    pub username: String,
    /// 密码哈希
    pub password_hash: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 用户角色
    pub role: String,
}

/// 创建新用户响应
#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    /// 用户 ID
    pub user_id: String,
}

/// 创建新用户
pub async fn create_user(
    headers: HeaderMap,
    req: Json<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain::domain();

    // 生成随机用户 ID
    let user_id = generate_id();

    // 创建 UserPo
    let role = UserRole::from_str(&req.role).unwrap_or(UserRole::Member);
    let user = UserPo::new(
        user_id.clone(),
        req.organization_id.clone(),
        req.username.clone(),
        req.display_name.clone().unwrap_or_default(),
        req.email.clone().unwrap_or_default(),
        req.password_hash.clone(),
        role,
        ctx.uid().clone(),
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
