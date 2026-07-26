//! 用户登录

use crate::middleware::jwt_auth::JWT_COOKIE_NAME;
use crate::pkg::RequestContext;
use crate::pkg::jwt;
use crate::service::domain::organization::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use common::api::ApiResponse;
use common::api::{LoginRequest, LoginResponse};
use common::error::Result;
use cookie::time;
use cookie::{Cookie, SameSite};

/// 用户登录
/// POST /organization/auth/login
pub async fn login(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse> {
    let domain = domain();

    // 验证用户名密码
    let user = domain
        .user_manage()
        .verify_password(ctx, &req.organization_id, &req.username, &req.password_hash)
        .await?;

    // 签发 JWT
    let token = jwt::encode_jwt(
        user.id.as_str(),
        user.username.as_str(),
        &req.organization_id,
        Some(user.role.to_i32()),
    )?;

    // 创建 Cookie（浏览器场景自动携带）
    let cookie = Cookie::build((JWT_COOKIE_NAME, token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(
            jwt::jwt_config().default_expiry_seconds(),
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
                user_id: user.id.clone(),
                username: user.username.clone(),
                organization_id: req.organization_id,
                token,
            })),
        ),
    ))
}
