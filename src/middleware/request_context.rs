//! RequestContext 提取中间件
//!
//! 自动从请求头提取 X-User-Id 和 X-User-Name，创建 RequestContext 并通过 Extension 注入

use crate::pkg::{RequestContext};
use axum::{
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
    body::Body,
};

/// RequestContext 提取中间件
///
/// 从请求头提取 X-User-Id 和 X-User-Name，创建 RequestContext 并注入到请求扩展中
pub async fn request_context_middleware(
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let user_id = headers
        .get("X-User-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    let user_name = headers
        .get("X-User-Name")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    let ctx = RequestContext::new(user_id, user_name);
    request.extensions_mut().insert(ctx);
    
    next.run(request).await
}
