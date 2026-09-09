//! 用户登出

use crate::middleware::jwt_auth::JWT_COOKIE_NAME;
use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use common::api::ApiResponse;
use common::api::LogoutResponse;
use common::error::Result;
use cookie::time;
use cookie::{Cookie, SameSite};

/// 用户登出
/// POST /organization/auth/logout
pub async fn logout(headers: HeaderMap) -> Result<impl IntoResponse> {
    // 清除 cookie，设置过期时间为 0
    // Secure 必须与签发时一致，否则 HTTPS 场景下浏览器不会回传清除指令、登出失效
    let cookie = Cookie::build((JWT_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .secure(
            crate::config::get()
                .server
                .cookie_secure(crate::middleware::proxy::forwarded_proto(&headers)),
        );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );

    Ok((
        headers,
        (
            StatusCode::OK,
            Json(ApiResponse::success(LogoutResponse { success: true })),
        ),
    ))
}
