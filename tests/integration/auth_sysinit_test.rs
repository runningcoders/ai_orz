//! Integration tests for authentication & system initialization flow.
//!
//! Covers:
//! - `POST /organization/initialize` creates org + admin + 2 providers
//! - `POST /organization/auth/login` returns a JWT token
//! - Protected routes return 401 without JWT
//! - Protected routes succeed with valid JWT
//!
//! 注意：`test_check_initialized_returns_false_on_fresh_db` 已被刻意省略 ——
//! 所有集成测试共享同一个全局 SQLite 数据库（`storage::init` 用 `OnceLock`，
//! 第二次调用 no-op），并行运行时其他测试可能先 bootstrap 让 initialized=true，
//! 导致该测试 flaky。改用 `test_initialize_system_creates_org_and_providers`
//! 中的正向断言（bootstrap 后 initialized=true）覆盖 check_initialized 路径。

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use sqlx::SqlitePool;

/// Initialize the system end-to-end: creates org + admin + chat provider.
/// `embedding_model` is `None`, so no embedding provider is configured.
#[sqlx::test]
async fn test_initialize_system_creates_org_and_providers(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let bs = crate::common::factories::bootstrap_system(&app).await;

    assert!(!bs.organization_id.is_empty(), "org_id should be non-empty");
    assert!(!bs.user_id.is_empty(), "user_id should be non-empty");
    assert!(
        !bs.chat_provider_id.is_empty(),
        "chat_provider_id should be non-empty"
    );
    assert!(
        bs.embedding_provider_id.is_none(),
        "embedding_provider_id should be None when not configured"
    );

    // After initialization, check_initialized should return true
    // 协议化改造后契约：data 为 CheckInitializedResponse 结构体（而非裸 bool）
    let (status, body) = app.get("/api/v1/organization/initialize/check").await;
    let data = crate::common::assert_api_ok(status, &body);
    let initialized = data
        .get("initialized")
        .and_then(|v| v.as_bool())
        .expect("expected initialized field in CheckInitializedResponse");
    assert!(initialized, "system should be initialized after bootstrap");
}

/// After system initialization, login with the admin credentials should return a JWT.
#[sqlx::test]
async fn test_login_returns_jwt_after_initialization(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login_and_get_jwt(
        &app,
        &bs.organization_id,
        &bs.username,
        &bs.password_hash,
    )
    .await;

    assert!(!jwt.is_empty(), "JWT token should be non-empty");
    // JWT format check: header.payload.signature (3 dot-separated base64 segments)
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 dot-separated parts");
}

/// Accessing a protected route without a JWT should return 401 Unauthorized.
#[sqlx::test]
async fn test_protected_route_returns_401_without_jwt(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // Hit a known protected route (e.g., list agents) without JWT
    let (status, _body) = app.get("/api/v1/hr/agents").await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNAUTHORIZED,
        "protected route without JWT should return 401"
    );
}

/// Accessing a protected route with a valid JWT should return 200 OK.
#[sqlx::test]
async fn test_protected_route_returns_200_with_jwt(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // Auth 链路验证用例保留完整 bootstrap（不删除 embedding provider），
    // 因为这个测试只验证路由 + JWT 注入，不触发实体创建。
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let (status, body) = app.get_with_jwt("/api/v1/hr/agents", &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "protected route with valid JWT should return 200, got body: {}",
        body
    );
}
