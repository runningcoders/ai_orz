//! 用户登出

use crate::error::AppError;
use crate::handlers::{ApiResponse};
use crate::middleware::jwt_auth::JWT_COOKIE_NAME;
use axum::{
    extract::Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use cookie::{Cookie, SameSite};
use cookie::time;
use serde::Serialize;

/// 登出响应
#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub success: bool,
}

/// 用户登出
/// POST /organization/auth/logout
pub async fn logout(
    _headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // 清除 cookie，设置过期时间为 0
    let cookie = Cookie::build((JWT_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .secure(false);

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );

    Ok((
        headers,
        (
            StatusCode::OK,
            Json(ApiResponse::success(LogoutResponse {
                success: true,
            })),
        ),
    ))
}
