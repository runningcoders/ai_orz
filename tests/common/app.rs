//! HTTP integration test app builder.
//!
//! Wraps `ai_orz::router::create_router` with a test `AppConfig` and
//! provides typed HTTP request helpers returning `(StatusCode, serde_json::Value)`.

use ai_orz::router::create_router;
use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use sqlx::SqlitePool;
use tower::ServiceExt;

/// Test application wrapping an axum Router with HTTP request helpers.
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
    pub async fn get(&self, path: &str) -> (StatusCode, serde_json::Value) {
        self.request(Method::GET, path, HeaderMap::new(), None).await
    }

    /// Issue a GET request with a JWT token (simulating authenticated browser session).
    pub async fn get_with_jwt(&self, path: &str, jwt: &str) -> (StatusCode, serde_json::Value) {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        self.request(Method::GET, path, headers, None).await
    }

    /// Issue a POST request with a JSON body.
    pub async fn post(&self, path: &str, body: &impl serde::Serialize) -> (StatusCode, serde_json::Value) {
        let body_json = serde_json::to_string(body).expect("failed to serialize request body");
        self.request(Method::POST, path, HeaderMap::new(), Some(body_json)).await
    }

    /// Issue a POST request with a JSON body and a JWT token.
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
            HeaderValue::from_str(&format!("orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(Method::POST, path, headers, Some(body_json)).await
    }

    /// Issue a PUT request with a JSON body and a JWT token.
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
            HeaderValue::from_str(&format!("orz_jwt={}", jwt))
                .expect("invalid JWT value for header"),
        );
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(Method::PUT, path, headers, Some(body_json)).await
    }

    /// Issue a DELETE request with a JWT token.
    pub async fn delete_with_jwt(&self, path: &str, jwt: &str) -> (StatusCode, serde_json::Value) {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("orz_jwt={}", jwt))
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
        let request = builder.body(request_body).expect("failed to build test request");
        let response = self.router.clone().oneshot(request).await.expect("test request failed");
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.expect("failed to read response body");
        let body_json: serde_json::Value = if body_bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
                serde_json::Value::String(String::from_utf8_lossy(&body_bytes).to_string())
            })
        };
        (status, body_json)
    }
}
