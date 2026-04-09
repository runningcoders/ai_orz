//! 用户登录

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::jwt;
use common::constants::RequestContext;
use crate::middleware::jwt_auth::JWT_COOKIE_NAME;
use crate::service::domain::organization::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use cookie::{Cookie, SameSite};
use cookie::time;
use serde::{Deserialize, Serialize};

/// 登录请求
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 密码哈希（前端已经 bcrypt 哈希）
    pub password_hash: String,
    /// 组织 ID
    pub organization_id: String,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 组织 ID
    pub organization_id: String,
}

/// 用户登录
/// POST /organization/auth/login
pub async fn login(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = domain();

    // 验证用户名密码
    let user = domain.user_manage().verify_password(
        ctx,
        &req.organization_id,
        &req.username,
        &req.password_hash,
    )?;

    // 签发 JWT
    let token = jwt::encode_jwt(
        &user.id,
        &user.username,
        &req.organization_id,
    )?;

    // 创建 Cookie
    let cookie = Cookie::build((JWT_COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(
            jwt::jwt_config().default_expiry_seconds()
        ))
        .secure(false); // 如果是 HTTPS 需要设置为 true

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );

    Ok((
        headers,
        (
            StatusCode::OK,
            Json(ApiResponse::success(LoginResponse {
                user_id: user.id,
                username: user.username,
                organization_id: req.organization_id,
            })),
        ),
    ))
}
