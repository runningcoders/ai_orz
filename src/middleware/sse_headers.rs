//! SSE 流式响应的防缓冲响应头
//!
//! nginx 默认开启 `proxy_buffering`，会把 `text/event-stream` 攒够一个缓冲块再下发，
//! 表现为前端 EventSource 长时间收不到事件（连 SSE 心跳也一起被吞）。
//! `X-Accel-Buffering: no` 是 nginx 的官方开关；Caddy / Traefik 本身不缓冲 SSE，
//! 收到该头也无害——因此无条件下发，与网关选型解耦。

use axum::{
    body::Body,
    http::{HeaderValue, Request, header},
    middleware::Next,
    response::Response,
};

/// 为 SSE 响应补 `X-Accel-Buffering: no` 与 `Cache-Control: no-cache`
pub async fn sse_headers_middleware(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let is_sse = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("text/event-stream"));
    if is_sse {
        let headers = response.headers_mut();
        headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::StatusCode, routing::get};
    use tower::ServiceExt;

    async fn sse_ok() -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::empty())
            .unwrap()
    }

    async fn json_ok() -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap()
    }

    fn app() -> Router {
        Router::new()
            .route("/sse", get(sse_ok))
            .route("/json", get(json_ok))
            .layer(axum::middleware::from_fn(sse_headers_middleware))
    }

    #[tokio::test]
    async fn sse_response_gets_no_buffering_headers() {
        let resp = app()
            .oneshot(Request::builder().uri("/sse").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("x-accel-buffering").unwrap(),
            "no",
            "SSE 必须关闭网关缓冲，否则事件会被攒包"
        );
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn non_sse_response_untouched() {
        let resp = app()
            .oneshot(Request::builder().uri("/json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(resp.headers().get("x-accel-buffering").is_none());
    }
}
