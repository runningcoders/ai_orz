//! JWT 认证中间件
//!
//! 从 Cookie 中提取 JWT token，验证后将用户信息注入到 RequestContext
//! 如果 token 不存在或验证失败，直接返回重定向到首页（引导登录）

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::{Response, Redirect, IntoResponse},
};
use crate::pkg::{jwt};
use common::constants::http_header;
use crate::error::AppError;

/// JWT cookie 名称
pub const JWT_COOKIE_NAME: &str = "ai_orz_jwt";

/// JWT 认证中间件
///
/// 从 Cookie 中提取 JWT token，验证后将用户信息添加到请求头
/// 验证失败直接返回 302 重定向到首页，引导用户到登录界面
pub async fn jwt_auth_middleware(
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // 1. 从 Cookie 中找到 JWT token
    let cookie_header = req.headers().get(axum::http::header::COOKIE);
    if cookie_header.is_none() {
        tracing::debug!("No Cookie header found, redirect to login");
        return Ok(Redirect::to("/").into_response());
    }

    let cookie_str = match cookie_header.unwrap().to_str() {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!("Invalid Cookie header, redirect to login");
            return Ok(Redirect::to("/").into_response());
        }
    };

    // 2. 查找 JWT cookie
    let mut token: Option<String> = None;
    for cookie in cookie::Cookie::split_parse(cookie_str) {
        if let Ok(cookie) = cookie {
            if cookie.name() == JWT_COOKIE_NAME {
                if !cookie.value().is_empty() {
                    token = Some(cookie.value().to_string());
                    break;
                }
            }
        }
    }

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            tracing::debug!("No JWT token found in cookie, redirect to login");
            return Ok(Redirect::to("/").into_response());
        }
    };

    // 3. 验证 JWT token
    let claims = match jwt::decode_jwt(&token) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("JWT token validation failed: {}, redirect to login", e);
            return Ok(Redirect::to("/").into_response());
        }
    };

    // 4. 将用户信息添加到请求头
    if !claims.user_id.is_empty() {
        if let Ok(header_value) = HeaderValue::from_str(&claims.user_id) {
            req.headers_mut().insert(
                http_header::USER_ID, header_value);
        }
    }
    if !claims.username.is_empty() {
        if let Ok(header_value) = HeaderValue::from_str(&claims.username) {
            req.headers_mut().insert(
                http_header::USERNAME, header_value);
        }
    }
    // 将组织 ID 添加到请求头（覆盖请求头中原有的值，以 JWT 中的为准）
    if !claims.organization_id.is_empty() {
        if let Ok(header_value) = HeaderValue::from_str(&claims.organization_id) {
            req.headers_mut().insert(
                http_header::ORGANIZATION_ID, header_value);
        }
    }

    // 5. JWT 验证通过，继续处理请求
    Ok(next.run(req).await)
}
