//! RequestContext 提取中间件
//!
//! 自动从请求头提取 X-User-Id 和 X-User-Name，创建 RequestContext 并通过 Extension 注入
//! 处理 LogId：如果请求头有则使用，否则自动生成，最终写回响应头

use crate::pkg::{RequestContext};
use crate::pkg::constants::http_header;
use axum::{
    http::{HeaderMap, HeaderValue, Request},
    middleware::Next,
    response::Response,
    body::Body,
};
use uuid::Uuid;

/// RequestContext 提取中间件
///
/// 从请求头提取 X-User-Id 和 X-User-Name，创建 RequestContext 并注入到请求扩展中
/// 处理 LogId：如果请求头有则使用，否则自动生成，最终写回响应头
pub async fn request_context_middleware(
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // 提取用户信息（使用常量）
    let user_id = headers
        .get(http_header::USER_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    let user_name = headers
        .get(http_header::USERNAME)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    // 处理 LogId：请求中有就用请求的，没有就自动生成
    let log_id = match headers
        .get(http_header::LOG_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    {
        Some(id) if !id.is_empty() => id,
        _ => Uuid::now_v7().to_string(),
    };
    
    let mut ctx = RequestContext::new(user_id, user_name);
    ctx.set_log_id(log_id);
    request.extensions_mut().insert(ctx.clone());
    
    // 处理请求并获取响应
    let mut response = next.run(request).await;
    
    // 将 LogId 写入响应头
    if let Ok(header_value) = HeaderValue::from_str(&ctx.log_id) {
        response.headers_mut().insert(http_header::LOG_ID, header_value);
    }
    
    response
}
