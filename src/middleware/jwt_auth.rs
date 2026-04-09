//! JWT 认证中间件
//!
//! 从 Cookie 中提取 JWT token，验证后将用户信息注入到 RequestContext

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use cookie::Cookie;
use crate::pkg::{jwt, RequestContext};
use crate::pkg::constants::http_header;

/// JWT cookie 名称
pub const JWT_COOKIE_NAME: &str = "ai_orz_jwt";

/// JWT 认证中间件
///
/// 从 Cookie 中提取 JWT token，验证后将用户信息添加到请求头
/// 这样后续的 handler 可以通过 RequestContext::from_headers 获取用户信息
pub async fn jwt_auth_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    // 1. 从 Cookie 中找到 JWT token
    if let Some(cookie_header) = req.headers().get(axum::http::header::COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            // 解析 cookie 找到 JWT
            for cookie in cookie::Cookie::split_parse(cookie_str) {
                if let Ok(cookie) = cookie {
                    if cookie.name() == JWT_COOKIE_NAME {
                        let token = cookie.value();
                        if !token.is_empty() {
                            // 2. 验证 JWT
                            match jwt::decode_jwt(token) {
                                Ok(claims) => {
                                    // 3. 将用户信息添加到请求头
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
                                    break;
                                }
                                Err(_) => {
                                    // JWT 无效，继续处理（不设置用户信息，保持未登录状态）
                                    tracing::debug!("JWT token invalid");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 继续处理请求
    next.run(req).await
}
