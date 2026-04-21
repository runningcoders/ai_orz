//! RequestContext 提取中间件
//!
//! 自动从请求头提取 X-User-Id 和 X-User-Name，创建 RequestContext 并通过 Extension 注入
//! 处理 LogId：如果请求头有则使用，否则自动生成，最终写回响应头

use axum::{
    body::Body,
    http::{HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use common::config::AppConfig;
use std::sync::Arc;
use crate::pkg::RequestContext;

/// RequestContext 提取中间件
///
/// 从请求头提取所有信息创建 RequestContext 并注入到请求扩展中
/// 处理 LogId：如果请求头有则使用，否则自动生成，最终写回响应头
pub async fn request_context_middleware(
    _config: Arc<AppConfig>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let headers = request.headers();
    let ctx = RequestContext::from_headers(headers);
    request.extensions_mut().insert(ctx.clone());

    // 处理请求并获取响应
    let mut response = next.run(request).await;

    // 将 LogId 写入响应头
    if let Ok(header_value) = HeaderValue::from_str(&ctx.log_id) {
        response
            .headers_mut()
            .insert(common::constants::http_header::LOG_ID, header_value);
    }

    response
}
