//! HTTP integration test app builder.
//!
//! Wraps `ai_orz::router::create_router` with a test `AppConfig` and
//! provides typed HTTP request helpers returning `(StatusCode, serde_json::Value)`.

use ai_orz::router::create_router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use sqlx::SqlitePool;
use std::time::Duration;
use tower::ServiceExt;

/// Test application wrapping an axum Router with HTTP request helpers.
#[derive(Clone)]
pub struct TestApp {
    router: axum::Router,
}

impl TestApp {
    /// Build a `TestApp` from the given SQLite pool.
    ///
    /// Caller must invoke `init_full_test_env(pool).await` before this,
    /// so that all DAO/DAL/Domain singletons are initialized.
    pub async fn new(_pool: SqlitePool) -> Self {
        // config::init() has already populated the global singleton in
        // init_full_test_env; we just fetch it for create_router.
        // (AppConfig doesn't implement Default; use the global instance.)
        let config = ai_orz::config::get();
        let router = create_router("", config);
        Self { router }
    }

    /// Issue a GET request. Returns (status, body_json).
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn get(&self, path: &str) -> (StatusCode, serde_json::Value) {
        self.request(Method::GET, path, HeaderMap::new(), None)
            .await
    }

    /// Issue a GET request with a JWT token (simulating authenticated browser session).
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn get_with_jwt(&self, path: &str, jwt: &str) -> (StatusCode, serde_json::Value) {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        self.request(Method::GET, path, headers, None).await
    }

    /// Issue a GET request with a JWT token, returning ONLY the status code.
    ///
    /// Use this for streaming endpoints (e.g. SSE) whose body never ends —
    /// `to_bytes(usize::MAX)` would hang forever waiting for EOF. We drop the
    /// response immediately after reading the status, which is enough for a
    /// connection-level smoke test.
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn get_with_jwt_status_only(&self, path: &str, jwt: &str) -> StatusCode {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        let mut builder = Request::builder().method(Method::GET).uri(path);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        let request = builder
            .body(Body::empty())
            .expect("failed to build test request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("test request failed");
        response.status()
    }

    /// Issue a POST request with a JSON body.
    pub async fn post(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> (StatusCode, serde_json::Value) {
        let body_json = serde_json::to_string(body).expect("failed to serialize request body");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(Method::POST, path, headers, Some(body_json))
            .await
    }

    /// Issue a POST request with a JSON body and a JWT token.
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn post_with_jwt(
        &self,
        path: &str,
        body: &impl serde::Serialize,
        jwt: &str,
    ) -> (StatusCode, serde_json::Value) {
        let body_json = serde_json::to_string(body).expect("failed to serialize request body");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(Method::POST, path, headers, Some(body_json))
            .await
    }

    /// Issue a PUT request with a JSON body and a JWT token.
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn put_with_jwt(
        &self,
        path: &str,
        body: &impl serde::Serialize,
        jwt: &str,
    ) -> (StatusCode, serde_json::Value) {
        let body_json = serde_json::to_string(body).expect("failed to serialize request body");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(Method::PUT, path, headers, Some(body_json))
            .await
    }

    /// Issue a DELETE request with a JWT token.
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn delete_with_jwt(&self, path: &str, jwt: &str) -> (StatusCode, serde_json::Value) {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        self.request(Method::DELETE, path, headers, None).await
    }

    /// Core request dispatcher.
    async fn request(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<String>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(path);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        let request_body = match body {
            Some(json) => Body::from(json),
            None => Body::empty(),
        };
        let request = builder
            .body(request_body)
            .expect("failed to build test request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("test request failed");
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read response body");
        let body_json: serde_json::Value = if body_bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
                serde_json::Value::String(String::from_utf8_lossy(&body_bytes).to_string())
            })
        };
        (status, body_json)
    }

    /// Connect to SSE endpoint and collect events for up to `max_wait` or
    /// until `max_events` have been received, whichever comes first.
    ///
    /// Returns `(status, Vec<parsed_event_json>)`. Each element is the parsed
    /// JSON payload from a `data: ...` line. ping/keep-alive lines are skipped.
    ///
    /// This is designed for SSE integration tests: connect, trigger some
    /// server-side action that produces a push, then verify the events arrived.
    #[allow(dead_code)] // 公共测试 API，保留供未来测试使用
    pub async fn get_with_jwt_collect_sse_events(
        &self,
        path: &str,
        jwt: &str,
        max_events: usize,
        max_wait: Duration,
    ) -> (StatusCode, Vec<serde_json::Value>) {
        use tokio::time::timeout;

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("ai_orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        let mut builder = Request::builder().method(Method::GET).uri(path);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        let request = builder
            .body(Body::empty())
            .expect("failed to build test request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("test request failed");
        let status = response.status();

        let body = response.into_body();
        let mut body_stream = body.into_data_stream();
        let mut collected: Vec<serde_json::Value> = Vec::with_capacity(max_events);
        let mut buffer = String::new();

        let deadline = tokio::time::Instant::now() + max_wait;
        while collected.len() < max_events {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let chunk = match timeout(remaining, body_stream.next()).await {
                Ok(Some(Ok(bytes))) => bytes,
                Ok(Some(Err(e))) => {
                    eprintln!("SSE stream error: {:?}", e);
                    break;
                }
                Ok(None) => break, // stream closed
                Err(_) => break,   // timeout
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE events (separated by blank line)
            while let Some(sep_idx) = buffer.find("\n\n") {
                let event_block = buffer[..sep_idx].to_string();
                buffer = buffer[sep_idx + 2..].to_string();

                let mut data_lines: Vec<String> = Vec::new();
                for line in event_block.lines() {
                    if let Some(rest) = line.strip_prefix("data:") {
                        let d = rest.trim();
                        // skip keep-alive / ping payloads
                        if !d.is_empty() && d != "keep-alive" {
                            data_lines.push(d.to_string());
                        }
                    }
                }
                if !data_lines.is_empty() {
                    let joined = data_lines.join("\n");
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&joined) {
                        collected.push(v);
                    }
                }
                if collected.len() >= max_events {
                    break;
                }
            }
        }
        (status, collected)
    }
}
