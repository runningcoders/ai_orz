//! JWT 认证中间件
//!
//! 双模式认证：优先从 Cookie 提取 JWT，fallback 从 Authorization: Bearer 提取
//! 验证后将用户信息写入请求头，后续 request_context_middleware 从请求头创建 RequestContext
//! - 浏览器请求（Cookie 模式）：验证失败返回 302 重定向到登录页
//! - API 调用（Bearer 模式）：验证失败返回 401 JSON

use crate::pkg::jwt;
use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Redirect, Response},
};
use common::api::ApiResponse;
use common::constants::http_header;
use common::error::Result;

/// JWT cookie 名称
pub const JWT_COOKIE_NAME: &str = "ai_orz_jwt";

/// Authorization header 中 Bearer 前缀
const BEARER_PREFIX: &str = "Bearer ";

/// JWT 认证中间件（双模式：Cookie + Bearer）
///
/// 认证顺序：
/// 1. 先从 Cookie 中查找 JWT token（浏览器场景）
/// 2. Cookie 没有则从 Authorization: Bearer 头查找（API 工具/代码调用场景）
///
/// 验证失败时的响应：
/// - 浏览器请求（有 Cookie 头或 Accept: text/html）→ 302 重定向到登录页
/// - API 请求（Bearer 模式）→ 401 JSON 错误
///
/// 注意：此中间件必须在 request_context_middleware 之前（外层）运行
pub async fn jwt_auth_middleware(mut req: Request, next: Next) -> Result<Response> {
    // 1. 提取 token：先 Cookie，后 Bearer
    let (token, is_browser) = extract_token(&req);

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(unauthorized_response(&req, is_browser));
        }
    };

    // 2. 验证 JWT token
    let claims = match jwt::decode_jwt(&token) {
        Ok(c) => c,
        Err(e) => {
            sys_debug!("JWT token validation failed: {}", e);
            return Ok(unauthorized_response(&req, is_browser));
        }
    };

    // 3. 将用户信息添加到请求头
    // 后续 request_context_middleware 会从请求头提取这些信息创建 RequestContext
    if !claims.user_id.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(&claims.user_id)
    {
        req.headers_mut().insert(http_header::USER_ID, header_value);
    }
    if !claims.username.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(&claims.username)
    {
        req.headers_mut()
            .insert(http_header::USERNAME, header_value);
    }
    if !claims.organization_id.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(&claims.organization_id)
    {
        req.headers_mut()
            .insert(http_header::ORGANIZATION_ID, header_value);
    }
    if let Some(role) = claims.role
        && let Ok(header_value) = HeaderValue::from_str(&role.to_string())
    {
        req.headers_mut()
            .insert(http_header::USER_ROLE, header_value);
    }
    // 注入 caller_type = User（JWT 验证通过的都是用户请求）
    req.headers_mut()
        .insert(http_header::CALLER_TYPE, HeaderValue::from_static("user"));

    // 4. JWT 验证通过，继续处理请求
    Ok(next.run(req).await)
}

/// 从请求中提取 JWT token
///
/// 返回 (token, is_browser)：
/// - 优先从 Cookie 提取（is_browser = true）
/// - Cookie 没有则从 Authorization: Bearer 提取（is_browser = false）
fn extract_token(req: &Request) -> (Option<String>, bool) {
    // 先从 Cookie 查找
    if let Some(cookie_header) = req.headers().get(axum::http::header::COOKIE)
        && let Ok(cookie_str) = cookie_header.to_str()
    {
        for cookie in cookie::Cookie::split_parse(cookie_str) {
            if let Ok(cookie) = cookie
                && cookie.name() == JWT_COOKIE_NAME
                && !cookie.value().is_empty()
            {
                return (Some(cookie.value().to_string()), true);
            }
        }
    }

    // Cookie 没有找到，从 Authorization: Bearer 查找
    if let Some(auth_header) = req.headers().get(axum::http::header::AUTHORIZATION)
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix(BEARER_PREFIX)
        && !token.is_empty()
    {
        return (Some(token.to_string()), false);
    }

    // 都没有找到，根据请求特征判断是否浏览器请求
    let is_browser = is_browser_request(req);
    (None, is_browser)
}

/// 判断是否是浏览器请求
fn is_browser_request(req: &Request) -> bool {
    // 有 Cookie 头 → 浏览器
    if req.headers().contains_key(axum::http::header::COOKIE) {
        return true;
    }
    // Accept 包含 text/html → 浏览器导航请求
    if let Some(accept) = req.headers().get(axum::http::header::ACCEPT)
        && let Ok(accept_str) = accept.to_str()
        && accept_str.contains("text/html")
    {
        return true;
    }
    false
}

/// 根据请求类型返回不同的未认证响应
fn unauthorized_response(_req: &Request, is_browser: bool) -> Response {
    if is_browser {
        sys_debug!("Unauthorized browser request, redirect to login");
        Redirect::to("/").into_response()
    } else {
        sys_debug!("Unauthorized API request, return 401");
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(
                401,
                "未认证或认证已过期".to_string(),
            )),
        )
            .into_response()
    }
}
