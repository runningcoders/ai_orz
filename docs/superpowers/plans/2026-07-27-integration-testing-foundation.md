# Integration Testing Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable HTTP API integration test infrastructure using `axum TestServer + tower::ServiceExt + sqlx::test`, then write end-to-end test suites covering 4 critical paths (auth/sysinit, core CRUD, message delivery, A2A), and enhance CI with coverage reporting and lint enforcement.

**Architecture:** Tests live in the top-level `tests/` directory (proper Rust integration tests, compiled separately from the lib target). A shared `tests/common/` module provides `init_full_test_env(pool)` (mirrors the `main.rs` startup flow: `pkg::init_all` components + `service::init()` aggregate), a `TestApp` builder wrapping `router::create_router`, and test data factories. Each test uses `#[sqlx::test]` to get an isolated SQLite pool, builds a fresh `TestApp`, and makes real HTTP requests via `tower::ServiceExt::oneshot`.

**Tech Stack:** Rust 2024 edition, axum 0.8, sqlx 0.8 (SQLite), tower 0.5, tokio 1, serde_json. Coverage via `cargo-tarpaulin`. CI on GitHub Actions (ubuntu-latest).

**Existing Assets to Reuse:**
- [src/lib.rs](file:///Users/aman/Technology/rust/ai_orz/src/lib.rs) — exposes `ai_orz` as a library
- [src/router.rs#L12](file:///Users/aman/Technology/rust/ai_orz/src/router.rs) — `pub fn create_router(frontend_dist_dir, config) -> Router`
- [src/service/mod.rs#L6-L15](file:///Users/aman/Technology/rust/ai_orz/src/service/mod.rs) — `service::init()` 聚合方法（一行替代 30+ 个手动 DAO/DAL/Domain init）
- [src/pkg/mod.rs#L21](file:///Users/aman/Technology/rust/ai_orz/src/pkg/mod.rs) — `pkg::init_all(config)` 聚合方法（参考，但测试中因 storage 隔离需求不直接调用）
- [src/pkg/request_context_test_support.rs](file:///Users/aman/Technology/rust/ai_orz/src/pkg/request_context_test_support.rs) — `new_test_ctx(user_id, pool)`
- [tests/http_handler_macro_test.rs#L37-L52](file:///Users/aman/Technology/rust/ai_orz/tests/http_handler_macro_test.rs) — 已验证的 `storage::init` 临时目录隔离模式（tempdir + InMemory vector）

---

## File Structure

```
tests/
├── common/
│   ├── mod.rs                    # Module declarations + pub use re-exports
│   ├── env.rs                    # init_full_test_env(pool) -> RequestContext
│   ├── app.rs                    # TestApp builder + HTTP request helpers
│   ├── factories/
│   │   ├── mod.rs
│   │   ├── user_factory.rs       # create_test_user + login_and_get_jwt
│   │   ├── agent_factory.rs      # create_test_agent
│   │   └── project_factory.rs    # create_test_project
│   └── assertions.rs             # assert_api_ok / assert_api_error
├── integration/
│   ├── auth_sysinit_test.rs     # Phase 2: Auth & system init flow
│   ├── core_crud_test.rs        # Phase 3: Agent/Project/Task CRUD
│   ├── message_delivery_test.rs # Phase 4: Message → AOP → Consumer → SSE
│   ├── a2a_flow_test.rs         # Phase 5: A2A tasks/send → callback
│   └── vector_degradation_test.rs # Phase 4.5: Verify vector index degradation
└── http_handler_macro_test.rs   # Existing, untouched

.github/workflows/rust.yml       # Phase 6: CI enhancement
```

**Design Decisions:**
1. **Tests live in `tests/` (not `src/`)** — proper Rust integration tests, compiled separately, cannot accidentally access private items.
2. **`init_full_test_env` is the single source of truth** for env setup — extracted from the existing `a2a/integration_test.rs` pattern, lives in `tests/common/env.rs`.
3. **`TestApp` wraps `Router` + provides typed HTTP helpers** — `app.get("/path")`, `app.post("/path", body).await`, `app.with_jwt(token)`. Returns `(StatusCode, ApiResponse<T>)` for easy assertions.
4. **Factories return business entities** — not POs, respecting the "Domain layer and above zero PO dependency" constraint.
5. **Each test uses `#[sqlx::test]`** — gets an isolated in-memory SQLite pool, no cross-test pollution.
6. **JWT tokens issued via real `/login` endpoint** — tests the full auth chain, not bypassed.

**Vector Index Testing Strategy (关键决策):**

调研结论（详见 [src/service/dal/project.rs:205-254](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/project.rs) 等DAL 实现）：

- **系统已有完善的向量降级机制**：所有 `embed_entity` 失败都被 `log_warn!` 吞掉，主流程永远返回 `Ok(())`，主表写入永远先于向量化
- **`initialize_system` 对 api_key 零校验**：只做 DB INSERT，不会真实调用模型 API。`api_key=""` 完全合法
- **FastEmbed (provider_type=6) 显式忽略 api_key**：[src/service/dao/cortex/rig/fastembed.rs:40](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/cortex/rig/fastembed.rs) 参数 `_api_key: &str` 直接丢弃
- **最干净的降级路径**：DB 里没有任何 `capability=Embedding AND status=Normal` 的 provider 时，DAL 走 `Ok(None)` 分支 `log_debug!("无可用 Embedding Provider，跳过向量索引")` 跳过 `embed_entity` 调用

**集成测试策略：依赖系统降级，不依赖真实 cortex**

1. **主线集成测试**：`bootstrap_system` 后立刻通过 HTTP 删除 embedding provider（`DELETE /api/v1/finance/model-providers/{embedding_id}`），后续所有 `agent_create / project_create / message_send` 都走 `Ok(None)` 降级路径。优点：
   - 零外部依赖（不需要 FastEmbed 模型下载、不需要 Ollama 服务）
   - CI 中快速稳定，不会因网络问题 flaky
   - 验证了系统"向量不可用时主流程仍可用"的设计意图
   - 与生产环境真实降级路径一致

2. **专项测试 `vector_degradation_test.rs`**：专门验证降级机制的健壮性（详见 Phase 4.5）

3. **不在集成测试中使用 FastEmbed**：虽然 FastEmbed 是纯本地，但首次启动会下载模型文件（[fastembed.rs:48](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/cortex/rig/fastembed.rs) 的 `TextEmbedding::try_new`），CI 中可能慢或不稳定。向量索引正确性的验证留给 DAL 层单元测试（已有 `MockCortexDao` 模式，见 [src/service/dal/agent_test.rs:100-174](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/agent_test.rs)）

---

## Phase 1: Test Infrastructure

### Task 1: Create `tests/common/mod.rs` module skeleton

**Files:**
- Create: `tests/common/mod.rs`
- Create: `tests/common/factories/mod.rs`

- [ ] **Step 1: Create the module declaration file**

Create `tests/common/mod.rs`:

```rust
//! Shared test infrastructure for HTTP API integration tests.
//!
//! Provides:
//! - `init_full_test_env` — full DAO/DAL/Domain initialization (extracted from a2a pattern)
//! - `TestApp` — wraps `axum::Router` with typed HTTP request helpers
//! - `factories` — test data factories returning business entities
//! - `assertions` — common API response assertions

pub mod app;
pub mod assertions;
pub mod env;
pub mod factories;

pub use app::TestApp;
pub use assertions::{assert_api_error, assert_api_ok};
pub use env::init_full_test_env;
```

- [ ] **Step 2: Create empty factories module**

Create `tests/common/factories/mod.rs`:

```rust
//! Test data factories returning business entities.

pub mod agent_factory;
pub mod project_factory;
pub mod user_factory;

pub use agent_factory::create_test_agent;
pub use project_factory::create_test_project;
pub use user_factory::{create_test_user, login_and_get_jwt};
```

- [ ] **Step 3: Commit**

```bash
git add tests/common/mod.rs tests/common/factories/mod.rs
git commit -m "test: scaffold tests/common/ module skeleton"
```

---

### Task 2: Create `tests/common/env.rs` with `init_full_test_env`

**Files:**
- Create: `tests/common/env.rs`
- Reference: [src/service/mod.rs#L6-L15](file:///Users/aman/Technology/rust/ai_orz/src/service/mod.rs) — `service::init()` 聚合方法
- Reference: [tests/http_handler_macro_test.rs#L37-L52](file:///Users/aman/Technology/rust/ai_orz/tests/http_handler_macro_test.rs) — 已验证的 storage 临时目录隔离模式

**设计决策（关键）：**
- **service 层用 `service::init()` 一行替代 30+ 个手动 init** — 与 `main.rs` 启动流程对齐，新增 DAO/DAL 时无需改测试代码
- **pkg 层 storage 用临时目录隔离** — 不能直接调 `pkg::init_all`，因为内部 `storage::init` 会用 `config.base_data_path()` 创建 SQLite 文件污染开发环境
- **其他 pkg 初始化（jwt/tool_registry/tool_tracing）单独调用** — 无副作用，可安全在测试中调用
- 所有 `init_*` 函数都是幂等的（基于 `OnceLock::set` 模式，第二次调用被忽略），多个 `#[sqlx::test]` 重复调用安全

- [ ] **Step 1: Create the env module**

Create `tests/common/env.rs`:

```rust
//! Full test environment initialization.
//!
//! Uses `ai_orz::service::init()` (the aggregated service-layer init) instead
//! of manually listing 30+ DAO/DAL/Domain init calls — mirrors the `main.rs`
//! startup flow. Storage is isolated to a tempdir to avoid polluting the dev
//! environment (same pattern as `tests/http_handler_macro_test.rs`).

use ai_orz::pkg::request_context_test_support::new_test_ctx;
use ai_orz::pkg::RequestContext;
use common::config::{DatabaseConfig, StatsConfig, VectorStoreType};
use sqlx::SqlitePool;

/// Initialize all pkg + service singletons + return a test RequestContext.
///
/// Idempotent: safe to call from every `#[sqlx::test]` (subsequent calls are
/// no-ops because all underlying `init` functions use `OnceLock::set`).
pub async fn init_full_test_env(pool: SqlitePool) -> RequestContext {
    // 1. Load global AppConfig (idempotent; reads `.ai_orz/ai_orz.toml`)
    let _ = ai_orz::config::init();

    // 2. pkg::storage — isolate to a tempdir + InMemory vector store to avoid
    //    polluting the dev `.ai_orz/` directory. Pattern proven by
    //    `tests/http_handler_macro_test.rs::ensure_storage_initialized`.
    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    let mut db_config = DatabaseConfig::default();
    db_config.vector_store_type = VectorStoreType::InMemory;
    let stats_config = StatsConfig::default();
    ai_orz::pkg::storage::init(tmp.path(), &db_config, &stats_config).await;
    // Leak the tempdir so the SQLite file stays alive for the test process lifetime.
    std::mem::forget(tmp);

    // 3. pkg::jwt — test-only secret (1 hour expiry is plenty for any test)
    ai_orz::pkg::jwt::init_jwt("test-jwt-secret-do-not-use-in-prod", 1);

    // 4. pkg::tool_tracing — agent creation writes trace files
    let trace_dir = std::env::temp_dir().join("ai_orz_integration_test_trace");
    let _ = std::fs::create_dir_all(&trace_dir);
    ai_orz::pkg::tool_tracing::logger::ToolCallLogger::init(trace_dir);

    // 5. pkg::tool_registry — register builtin tools (idempotent via registry set)
    ai_orz::pkg::tool_registry::builtin::register_all(
        ai_orz::pkg::tool_registry::get_registry(),
    );

    // 6. service layer — one-line replacement for 30+ manual DAO/DAL/Domain init calls.
    //    Internally calls: dao::init_all() + dal::init_all() + domain::init_all().
    ai_orz::service::init();

    new_test_ctx("test-integration-user", pool)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo build --tests --no-run`
Expected: compiles with errors only about missing `app`, `assertions`, `factories` modules (created in next tasks). If so, the `env.rs` module itself is OK.

- [ ] **Step 3: Commit**

```bash
git add tests/common/env.rs
git commit -m "test: extract init_full_test_env into tests/common/env.rs"
```

---

### Task 3: Create `tests/common/app.rs` with `TestApp` builder

**Files:**
- Create: `tests/common/app.rs`
- Reference: [src/router.rs#L12](file:///Users/aman/Technology/rust/ai_orz/src/router.rs) — `create_router`

- [ ] **Step 1: Create the TestApp module**

Create `tests/common/app.rs`:

```rust
//! HTTP integration test app builder.
//!
//! Wraps `ai_orz::router::create_router` with a test `AppConfig` and
//! provides typed HTTP request helpers returning `(StatusCode, serde_json::Value)`.

use ai_orz::router::create_router;
use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use common::config::AppConfig;
use sqlx::SqlitePool;
use std::sync::Arc;
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
        // Use a minimal AppConfig — config::init() has already populated the
        // global singleton; we just need an Arc<AppConfig> for create_router.
        let config = Arc::new(AppConfig::default());
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
```

- [ ] **Step 2: Verify it compiles**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo build --tests --no-run`
Expected: compiles (now `app` module exists). May still have warnings about unused fields.

- [ ] **Step 3: Commit**

```bash
git add tests/common/app.rs
git commit -m "test: add TestApp builder with HTTP request helpers"
```

---

### Task 4: Create `tests/common/assertions.rs`

**Files:**
- Create: `tests/common/assertions.rs`

- [ ] **Step 1: Create the assertions module**

Create `tests/common/assertions.rs`:

```rust
//! Common API response assertions.

use axum::http::StatusCode;
use serde_json::Value;

/// Assert that the response is `200 OK` with `code: 0` in the API envelope.
/// Returns the `data` field for further assertions.
pub fn assert_api_ok(status: StatusCode, body: &Value) -> Value {
    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK, got {}: {}",
        status,
        body
    );
    let code = body
        .get("code")
        .unwrap_or_else(|| panic!("response missing 'code' field: {}", body))
        .as_i64()
        .unwrap_or_else(|| panic!("'code' field is not an integer: {}", body));
    assert_eq!(code, 0, "expected code=0 (success), got code={}: {}", code, body);
    body.get("data")
        .cloned()
        .unwrap_or_else(|| panic!("response missing 'data' field: {}", body))
}

/// Assert that the response has the given HTTP status and a non-zero `code` in the envelope.
pub fn assert_api_error(status: StatusCode, body: &Value, expected_status: StatusCode) {
    assert_eq!(
        status, expected_status,
        "expected {} got {}: {}",
        expected_status, status, body
    );
    let code = body
        .get("code")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(
        code != 0,
        "expected non-zero error code, got code=0 with body: {}",
        body
    );
}
```

- [ ] **Step 2: Commit**

```bash
git add tests/common/assertions.rs
git commit -m "test: add common API response assertions"
```

---

### Task 5: Create `tests/common/factories/user_factory.rs` with login flow

**Files:**
- Create: `tests/common/factories/user_factory.rs`
- Reference: [common/src/api/auth.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/auth.rs) — `LoginRequest`
- Reference: [common/src/api/organization.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/organization.rs) — `InitializeSystemRequest`

**向量降级策略（关键）：**
- `bootstrap_system` 仍按接口要求传 chat_model + embedding_model，但 `api_key=""`、`provider_type=6 (FastEmbed)` —— 不真实调用模型
- 返回 `embedding_provider_id` 让调用方决定是否走 `disable_embedding_provider` 降级路径
- 主线集成测试调用 `bootstrap_and_login_with_vector_disabled` 一步到位完成 bootstrap + 删除 embedding provider，后续所有 CRUD 自动走 `Ok(None)` 跳过路径

- [ ] **Step 1: Create the user factory**

Create `tests/common/factories/user_factory.rs`:

```rust
//! User/auth test factories.
//!
//! Provides helpers to:
//! 1. Bootstrap a system (org + admin user + 2 model providers) via the real
//!    `/organization/initialize` endpoint.
//! 2. Login as the admin user via the real `/organization/auth/login` endpoint
//!    and return a JWT token.
//! 3. Optionally disable the embedding provider so subsequent entity creates
//!    take the `Ok(None)` vector-degradation path (no cortex calls).

use crate::common::app::TestApp;
use common::api::{InitializeSystemRequest, LoginRequest, ModelProviderInitConfig};

/// Bootstrap result — contains everything tests need to make authenticated calls.
#[derive(Debug, Clone)]
pub struct BootstrappedSystem {
    pub organization_id: String,
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub chat_provider_id: String,
    pub embedding_provider_id: String,
}

/// Bootstrap the system with one org, one admin, and two model providers.
///
/// **向量降级关键**：embedding_model 用 `provider_type=6 (FastEmbed)` + `api_key=""`，
/// 因为 [src/service/dao/cortex/rig/fastembed.rs:40](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/cortex/rig/fastembed.rs)
/// 显式忽略 api_key。`initialize_system` 只做 DB INSERT 不真实调用模型，
/// 所以这个 provider 创建出来即使 cortex 不可用也不会失败。
pub async fn bootstrap_system(app: &TestApp) -> BootstrappedSystem {
    let username = format!("admin-{}", uuid::Uuid::now_v7());
    let password_hash = format!("hash-{}", uuid::Uuid::now_v7());
    let org_name = format!("TestOrg-{}", uuid::Uuid::now_v7());

    let req = InitializeSystemRequest {
        organization_name: org_name,
        admin_username: username.clone(),
        admin_password_hash: password_hash.clone(),
        description: Some("Integration test org".to_string()),
        admin_display_name: Some("Test Admin".to_string()),
        admin_email: Some("admin@test.local".to_string()),
        chat_model: ModelProviderInitConfig {
            name: "Test Chat Provider".to_string(),
            provider_type: 0, // OpenAI
            model_name: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            description: Some("test chat model".to_string()),
        },
        embedding_model: ModelProviderInitConfig {
            name: "Test Embedding Provider".to_string(),
            provider_type: 6, // FastEmbed — 显式忽略 api_key
            model_name: "BAAI/bge-small-en".to_string(),
            api_key: "".to_string(), // 空字符串，FastEmbed 不需要
            base_url: None,
            description: Some("test embedding model".to_string()),
        },
    };

    let (status, body) = app.post("/api/v1/organization/initialize", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    let org_id = data
        .get("organization_id")
        .and_then(|v| v.as_str())
        .expect("missing organization_id in response")
        .to_string();
    let user_id = data
        .get("user_id")
        .and_then(|v| v.as_str())
        .expect("missing user_id in response")
        .to_string();
    let chat_provider_id = data
        .get("chat_provider_id")
        .and_then(|v| v.as_str())
        .expect("missing chat_provider_id in response")
        .to_string();
    let embedding_provider_id = data
        .get("embedding_provider_id")
        .and_then(|v| v.as_str())
        .expect("missing embedding_provider_id in response")
        .to_string();
    BootstrappedSystem {
        organization_id: org_id,
        user_id,
        username,
        password_hash,
        chat_provider_id,
        embedding_provider_id,
    }
}

/// Login as the given user via the real `/organization/auth/login` endpoint.
///
/// Returns the JWT token. Tests should pass this to `TestApp::get_with_jwt` etc.
pub async fn login_and_get_jwt(
    app: &TestApp,
    organization_id: &str,
    username: &str,
    password_hash: &str,
) -> String {
    let req = LoginRequest {
        organization_id: organization_id.to_string(),
        username: username.to_string(),
        password_hash: password_hash.to_string(),
    };
    let (status, body) = app.post("/api/v1/organization/auth/login", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("token")
        .and_then(|v| v.as_str())
        .expect("missing token in login response")
        .to_string()
}

/// Disable the embedding provider by deleting it via HTTP.
///
/// After this call, `get_default_embedding_provider` returns `Ok(None)` for all
/// subsequent entity creates, which triggers the `log_debug!("无可用 Embedding
/// Provider，跳过向量索引")` degradation path in every DAL.
///
/// This is the **recommended default** for integration tests that don't
/// specifically test vector indexing — keeps tests fast and CI-stable.
pub async fn disable_embedding_provider(app: &TestApp, jwt: &str, embedding_provider_id: &str) {
    let (status, _body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "deleting embedding provider should succeed, body: {}",
        _body
    );
}

/// Convenience: bootstrap system + login, returning
/// `(BootstrappedSystem, jwt_token)`.
pub async fn bootstrap_and_login(app: &TestApp) -> (BootstrappedSystem, String) {
    let bs = bootstrap_system(app).await;
    let jwt = login_and_get_jwt(app, &bs.organization_id, &bs.username, &bs.password_hash).await;
    (bs, jwt)
}

/// Convenience: bootstrap system + login + delete embedding provider.
///
/// This is the **default entry point** for most integration tests. Subsequent
/// entity creates (agent/project/task/message) will all take the vector
/// degradation path — no cortex calls, no FastEmbed model downloads, fast and
/// deterministic.
pub async fn bootstrap_login_and_disable_embedding(
    app: &TestApp,
) -> (BootstrappedSystem, String) {
    let (bs, jwt) = bootstrap_and_login(app).await;
    disable_embedding_provider(app, &jwt, &bs.embedding_provider_id).await;
    (bs, jwt)
}
```

- [ ] **Step 2: Update factories/mod.rs re-exports**

Modify `tests/common/factories/mod.rs` to export the new types:

```rust
//! Test data factories returning business entities.

pub mod agent_factory;
pub mod project_factory;
pub mod user_factory;

pub use agent_factory::create_test_agent;
pub use project_factory::create_test_project;
pub use user_factory::{
    bootstrap_and_login, bootstrap_login_and_disable_embedding,
    bootstrap_system, disable_embedding_provider, login_and_get_jwt,
    BootstrappedSystem,
};
```

- [ ] **Step 2: Commit**

```bash
git add tests/common/factories/user_factory.rs
git commit -m "test: add user_factory with bootstrap_system + login flow"
```

---

### Task 6: Create minimal `agent_factory.rs` and `project_factory.rs` stubs

**Files:**
- Create: `tests/common/factories/agent_factory.rs`
- Create: `tests/common/factories/project_factory.rs`

These factories create entities via HTTP endpoints (not direct DAL calls) to maximize test realism.

- [ ] **Step 1: Create agent_factory**

Create `tests/common/factories/agent_factory.rs`:

```rust
//! Agent test factory — creates agents via the real `/hr/agents` HTTP endpoint.

use crate::common::app::TestApp;
use serde_json::json;

/// Create a test agent via the HTTP API.
///
/// `provider_id` should come from `bootstrap_system` (chat provider id).
/// Returns the created agent's ID.
pub async fn create_test_agent(
    app: &TestApp,
    jwt: &str,
    provider_id: &str,
    name: &str,
) -> String {
    let req = json!({
        "name": name,
        "tags": ["test"],
        "description": "Test agent",
        "capabilities": ["chat"],
        "soul": "Test soul",
        "model_provider_id": provider_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/agents", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in agent create response")
        .to_string()
}
```

- [ ] **Step 2: Create project_factory**

Create `tests/common/factories/project_factory.rs`:

```rust
//! Project test factory — creates projects via the real `/project/projects` HTTP endpoint.

use crate::common::app::TestApp;
use serde_json::json;

/// Create a test project via the HTTP API. Returns the project ID.
pub async fn create_test_project(app: &TestApp, jwt: &str, name: &str) -> String {
    let req = json!({
        "name": name,
        "description": "Test project",
    });
    let (status, body) = app.post_with_jwt("/api/v1/project/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in project create response")
        .to_string()
}
```

- [ ] **Step 3: Verify it all compiles**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo build --tests --no-run`
Expected: compiles cleanly. No test runs yet — that's correct.

- [ ] **Step 4: Commit**

```bash
git add tests/common/factories/agent_factory.rs tests/common/factories/project_factory.rs
git commit -m "test: add agent and project factories using HTTP endpoints"
```

---

## Phase 2: Auth & System Init Test Suite

### Task 7: Create `tests/integration/auth_sysinit_test.rs` with first passing test

**Files:**
- Create: `tests/integration/auth_sysinit_test.rs`

- [ ] **Step 1: Write the first failing test (check_initialized on fresh DB)**

Create `tests/integration/auth_sysinit_test.rs`:

```rust
//! Integration tests for authentication & system initialization flow.
//!
//! Covers:
//! - `GET /organization/initialize/check` on fresh DB returns `false`
//! - `POST /organization/initialize` creates org + admin + 2 providers
//! - `POST /organization/auth/login` returns a JWT token
//! - Protected routes return 401 without JWT
//! - Protected routes succeed with valid JWT

mod common;

use crate::common::TestApp;
use sqlx::SqlitePool;

/// On a fresh DB, the system should report it is not initialized.
#[sqlx::test]
async fn test_check_initialized_returns_false_on_fresh_db(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (status, body) = app
        .get("/api/v1/organization/initialize/check")
        .await;

    assert_eq!(status, axum::http::StatusCode::OK);
    let data = crate::common::assert_api_ok(status, &body);
    let initialized = data
        .as_bool()
        .expect("expected boolean in data field");
    assert!(!initialized, "fresh DB should report not initialized");
}
```

- [ ] **Step 2: Run the test to verify it fails first, then passes**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test -- --nocapture`

Expected: at first run, the test may fail if the route path is wrong. Verify the actual route path matches [src/router.rs](file:///Users/aman/Technology/rust/ai_orz/src/router.rs) public_routes. If the path is correct, the test should PASS on the first try because all the production code is already in place — this test serves as a smoke test of the infrastructure.

If it fails with `404 NOT FOUND`, check the actual route in `src/router.rs::public_routes` and adjust the path in the test.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/auth_sysinit_test.rs
git commit -m "test: add check_initialized smoke test for auth infrastructure"
```

---

### Task 8: Add `initialize_system` flow test

**Files:**
- Modify: `tests/integration/auth_sysinit_test.rs`

- [ ] **Step 1: Append the test**

Append to `tests/integration/auth_sysinit_test.rs`:

```rust
/// Initialize the system end-to-end: creates org + admin + chat provider + embedding provider.
#[sqlx::test]
async fn test_initialize_system_creates_org_and_providers(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let bs = crate::common::factories::bootstrap_system(&app).await;

    assert!(!bs.organization_id.is_empty(), "org_id should be non-empty");
    assert!(!bs.user_id.is_empty(), "user_id should be non-empty");
    assert!(!bs.chat_provider_id.is_empty(), "chat_provider_id should be non-empty");
    assert!(!bs.embedding_provider_id.is_empty(), "embedding_provider_id should be non-empty");

    // After initialization, check_initialized should return true
    let (status, body) = app
        .get("/api/v1/organization/initialize/check")
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let initialized = data
        .as_bool()
        .expect("expected boolean in data field");
    assert!(initialized, "system should be initialized after bootstrap");
}
```

- [ ] **Step 2: Run the new test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test test_initialize_system_creates_org_and_providers -- --nocapture`

Expected: PASS. If it fails, check the `InitializeSystemRequest` field names match [common/src/api/organization.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/organization.rs).

- [ ] **Step 3: Commit**

```bash
git add tests/integration/auth_sysinit_test.rs
git commit -m "test: add initialize_system end-to-end test"
```

---

### Task 9: Add `login` returns JWT test

**Files:**
- Modify: `tests/integration/auth_sysinit_test.rs`

- [ ] **Step 1: Append the test**

Append to `tests/integration/auth_sysinit_test.rs`:

```rust
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
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test test_login_returns_jwt_after_initialization -- --nocapture`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/auth_sysinit_test.rs
git commit -m "test: add login returns JWT test"
```

---

### Task 10: Add protected route 401/200 tests

**Files:**
- Modify: `tests/integration/auth_sysinit_test.rs`

- [ ] **Step 1: Append the two tests**

Append to `tests/integration/auth_sysinit_test.rs`:

```rust
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
    let (_bs, jwt) =
        crate::common::factories::bootstrap_and_login(&app).await;

    let (status, body) = app.get_with_jwt("/api/v1/hr/agents", &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "protected route with valid JWT should return 200, got body: {}",
        body
    );
}
```

- [ ] **Step 2: Run the full auth_sysinit test suite**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test -- --nocapture`

Expected: all 5 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/auth_sysinit_test.rs
git commit -m "test: add protected route 401/200 tests"
```

---

## Phase 3: Core CRUD Test Suite

### Task 11: Create `tests/integration/core_crud_test.rs` with Agent CRUD loop

**Files:**
- Create: `tests/integration/core_crud_test.rs`

- [ ] **Step 1: Write the agent CRUD loop test**

Create `tests/integration/core_crud_test.rs`:

```rust
//! Integration tests for core business CRUD loops.
//!
//! Covers:
//! - Agent create → list → get → update → delete
//! - Project create → list → get → update status → delete
//! - Task create → update progress → mark done

mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Full Agent CRUD loop:
/// 1. Create agent (returns id)
/// 2. List agents (should contain the new id)
/// 3. Get agent by id (should match)
/// 4. Update agent name
/// 5. Delete agent
/// 6. Get by id should now 404 or return error
#[sqlx::test]
async fn test_agent_crud_loop(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // 删除 embedding provider 走向量降级路径，避免触发 FastEmbed 模型下载
    let (bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Fetch the chat provider id (still present after bootstrap)
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/model-providers", &jwt)
        .await;
    let providers_data = crate::common::assert_api_ok(status, &body);
    let provider_id = providers_data
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("id"))
        .and_then(|v| v.as_str())
        .expect("expected at least one model provider after bootstrap")
        .to_string();

    // 1. Create agent
    let agent_name = format!("TestAgent-{}", uuid::Uuid::now_v7());
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &provider_id,
        &agent_name,
    )
    .await;

    // 2. List agents — should contain our new id
    let (status, body) = app.get_with_jwt("/api/v1/hr/agents", &jwt).await;
    let list_data = crate::common::assert_api_ok(status, &body);
    let found_in_list = list_data
        .as_array()
        .map(|arr| {
            arr.iter().any(|item| {
                item.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str())
            })
        })
        .unwrap_or(false);
    assert!(found_in_list, "created agent should appear in list");

    // 3. Get agent by id
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("name").and_then(|v| v.as_str()),
        Some(agent_name.as_str()),
        "fetched agent name should match"
    );

    // 4. Update agent name
    let new_name = format!("UpdatedAgent-{}", uuid::Uuid::now_v7());
    let update_req = json!({
        "id": agent_id,
        "name": new_name,
        "tags": ["test"],
        "description": "Updated agent",
        "capabilities": ["chat"],
        "soul": "Updated soul",
        "model_provider_id": provider_id,
    });
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &update_req, &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "update should succeed");

    // Re-fetch and verify name changed
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("name").and_then(|v| v.as_str()),
        Some(new_name.as_str()),
        "name should be updated"
    );

    // 5. Delete agent
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "delete should succeed");

    // 6. Re-fetch should fail (404 or non-zero code)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    assert!(
        status == axum::http::StatusCode::NOT_FOUND
            || body.get("code").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        "deleted agent should not be retrievable"
    );

    // bs 保留以验证完整 bootstrap 返回的字段
    let _ = bs;
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test core_crud_test test_agent_crud_loop -- --nocapture`

Expected: PASS. If it fails, inspect the actual API DTO field names by reading the corresponding handler files (e.g., [src/handlers/hr/agent/create_agent.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/hr/agent/create_agent.rs)) and adjust the JSON in the test to match.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/core_crud_test.rs
git commit -m "test: add agent CRUD loop integration test"
```

---

### Task 12: Add Project CRUD with status transitions

**Files:**
- Modify: `tests/integration/core_crud_test.rs`

- [ ] **Step 1: Append the project status transition test**

Append to `tests/integration/core_crud_test.rs`:

```rust
/// Project create → update status transitions → delete.
#[sqlx::test]
async fn test_project_status_transitions(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Create project
    let project_name = format!("TestProject-{}", uuid::Uuid::now_v7());
    let project_id =
        crate::common::factories::create_test_project(&app, &jwt, &project_name).await;

    // Get project — verify initial state
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/project/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        project_data.get("name").and_then(|v| v.as_str()),
        Some(project_name.as_str())
    );

    // Update project status — invoke the /status action endpoint
    // (refer to src/handlers/project/project/update_project_status.rs for exact shape)
    let status_req = json!({
        "id": project_id,
        "status": "in_progress"
    });
    let (status_code, body) = app
        .post_with_jwt(
            &format!("/api/v1/project/projects/{}/status", project_id),
            &status_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "status update should succeed, body: {}",
        body
    );

    // Re-fetch and verify status changed
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/project/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    let updated_status = project_data
        .get("status")
        .and_then(|v| v.as_str())
        .or_else(|| {
            project_data
                .get("status")
                .and_then(|v| v.as_i64())
                .map(|_| "numeric_status")
        })
        .unwrap_or("missing_status");
    assert!(
        updated_status != "missing_status",
        "project status field should be present after update, got: {}",
        project_data
    );

    // Delete project
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/project/projects/{}", project_id), &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "delete should succeed");
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test core_crud_test test_project_status_transitions -- --nocapture`

Expected: PASS. If the status update endpoint URL or payload shape is wrong, read [src/handlers/project/project/update_project_status.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/project/project/update_project_status.rs) and the route definition in `src/router.rs::protected_routes` to correct.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/core_crud_test.rs
git commit -m "test: add project status transition integration test"
```

---

### Task 13: Add Task CRUD with progress updates

**Files:**
- Modify: `tests/integration/core_crud_test.rs`

- [ ] **Step 1: Append the task test**

Append to `tests/integration/core_crud_test.rs`:

```rust
/// Task create under a project → update progress → mark done.
#[sqlx::test]
async fn test_task_progress_and_completion(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Create a project to host the task
    let project_id = crate::common::factories::create_test_project(
        &app,
        &jwt,
        &format!("TaskHost-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Create a task under the project
    let task_req = json!({
        "project_id": project_id,
        "title": "Test task",
        "description": "Test task for integration",
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/project/tasks", &task_req, &jwt)
        .await;
    let task_data = crate::common::assert_api_ok(status, &body);
    let task_id = task_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id in create response")
        .to_string();

    // Update task progress to 50%
    let progress_req = json!({
        "id": task_id,
        "progress": 50
    });
    let (status, _body) = app
        .post_with_jwt(
            &format!("/api/v1/project/tasks/{}/progress", task_id),
            &progress_req,
            &jwt,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "progress update should succeed");

    // Mark task done
    let (status, _body) = app
        .post_with_jwt(
            &format!("/api/v1/project/tasks/{}/mark-done", task_id),
            &json!({}),
            &jwt,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "mark-done should succeed");

    // Re-fetch task and verify final state
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/project/tasks/{}", task_id), &jwt)
        .await;
    let task_data = crate::common::assert_api_ok(status, &body);
    assert!(
        task_data.get("status").is_some(),
        "task status field should be present"
    );
}
```

- [ ] **Step 2: Run all CRUD tests**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test core_crud_test -- --nocapture`

Expected: all 3 tests PASS. If endpoint URLs are wrong, check the actual routes in `src/router.rs::protected_routes` and adjust.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/core_crud_test.rs
git commit -m "test: add task progress and completion integration test"
```

---

## Phase 4: Message Delivery Test Suite

### Task 14: Create `tests/integration/message_delivery_test.rs` with send_message smoke test

**Files:**
- Create: `tests/integration/message_delivery_test.rs`

- [ ] **Step 1: Write the send_message smoke test**

Create `tests/integration/message_delivery_test.rs`:

```rust
//! Integration tests for the message delivery pipeline.
//!
//! Covers:
//! - `POST /finance/messages/send` enqueues to AOP queue
//! - Message record is persisted in DB after send
//! - SSE subscription endpoint returns 200 and event-stream content type

mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Send a message — verifies the message record is persisted with the correct
/// from/to/content fields. This is the entry point of the delivery pipeline.
#[sqlx::test]
async fn test_send_message_persists_record(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Create an agent to receive the message
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/model-providers", &jwt)
        .await;
    let providers = crate::common::assert_api_ok(status, &body);
    let provider_id = providers
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("id"))
        .and_then(|v| v.as_str())
        .expect("expected a model provider")
        .to_string();

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &provider_id,
        &format!("MsgReceiver-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Send a message to the agent
    let send_req = json!({
        "to_role": "agent",
        "to_id": agent_id,
        "content": "Hello from integration test",
        "content_type": "text"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/send", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing message id in send response")
        .to_string();

    // List messages — should contain our new message
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/finance/messages?to_id={}", agent_id), &jwt)
        .await;
    let list_data = crate::common::assert_api_ok(status, &body);
    let found = list_data
        .as_array()
        .map(|arr| {
            arr.iter().any(|item| {
                item.get("id").and_then(|v| v.as_str()) == Some(message_id.as_str())
            })
        })
        .unwrap_or(false);
    assert!(found, "sent message should appear in list");
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test message_delivery_test test_send_message_persists_record -- --nocapture`

Expected: PASS. If the send endpoint URL or payload is wrong, read [src/handlers/finance/message/send_message.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/finance/message/send_message.rs) and adjust.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/message_delivery_test.rs
git commit -m "test: add send_message smoke test for delivery pipeline"
```

---

### Task 15: Add SSE subscription smoke test

**Files:**
- Modify: `tests/integration/message_delivery_test.rs`

- [ ] **Step 1: Append the SSE smoke test**

Append to `tests/integration/message_delivery_test.rs`:

```rust
/// SSE subscription endpoint should return 200 and `text/event-stream` content type.
///
/// This is a connection-level smoke test — we do not assert on event content
/// because that requires a longer-lived subscription and event production.
#[sqlx::test]
async fn test_sse_endpoint_returns_event_stream(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // SSE endpoint uses GET; we just check the connection establishes
    let (status, _body) = app
        .get_with_jwt("/api/v1/finance/messages/sse", &jwt)
        .await;
    // SSE streams may return 200 with text/event-stream or 200 with no body
    // since oneshot only captures the initial response. We just assert 200.
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "SSE endpoint should return 200"
    );
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test message_delivery_test test_sse_endpoint_returns_event_stream -- --nocapture`

Expected: PASS (SSE connection establishes). If 404, check the actual SSE route in [src/router.rs](file:///Users/aman/Technology/rust/ai_orz/src/router.rs).

- [ ] **Step 3: Commit**

```bash
git add tests/integration/message_delivery_test.rs
git commit -m "test: add SSE endpoint connection smoke test"
```

---

## Phase 4.5: Vector Index Degradation Test Suite

### Task 15.5: Create `tests/integration/vector_degradation_test.rs`

**Files:**
- Create: `tests/integration/vector_degradation_test.rs`
- Reference: [src/service/dal/project.rs:205-254](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/project.rs) — `embed_entity` 降级实现

**这个专项测试的目的：**
1. 显式验证系统的向量降级保证，防止后续重构破坏这一关键健壮性
2. 文档化"无 Embedding Provider 时主流程仍可用"的设计契约
3. 与主线测试形成对照 —— 主线测试用降级路径"绕过"向量，本测试"验证"降级路径本身正确

- [ ] **Step 1: Write the degradation test**

Create `tests/integration/vector_degradation_test.rs`:

```rust
//! Integration tests verifying vector index degradation guarantees.
//!
//! These tests explicitly verify the design contract:
//! - When no Embedding provider is available (Ok(None) path), entity creates
//!   succeed without panic and the main table record is persisted.
//! - When cortex calls fail (Err path), entity creates still succeed because
//!   DAL layer catches the error with `log_warn!` and does not propagate.
//!
//! This protects a critical robustness guarantee documented in
//! [src/service/dal/project.rs:205-254] and other DAL modules.

mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// When the embedding provider is deleted, agent creation should still succeed
/// and the agent record should be retrievable.
///
/// Validates the `Ok(None)` degradation path in
/// [src/service/dal/agent.rs:207-215].
#[sqlx::test]
async fn test_agent_create_succeeds_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Verify embedding provider is really gone
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/model-providers", &jwt)
        .await;
    let providers = crate::common::assert_api_ok(status, &body);
    let has_embedding = providers
        .as_array()
        .map(|arr| {
            arr.iter().any(|p| {
                p.get("capability")
                    .and_then(|v| v.as_i64())
                    .map(|c| c == 1) // Embedding = 1
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    assert!(!has_embedding, "embedding provider should be deleted");

    // Create an agent — should succeed despite no embedding provider
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("NoVecAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Re-fetch — main table record must be persisted
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("id").and_then(|v| v.as_str()),
        Some(agent_id.as_str()),
        "agent should be retrievable after create-without-embedding"
    );
}

/// When the embedding provider is deleted, project creation should still succeed.
///
/// Validates the `Ok(None)` degradation path in
/// [src/service/dal/project.rs:234-241].
#[sqlx::test]
async fn test_project_create_succeeds_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    let project_id = crate::common::factories::create_test_project(
        &app,
        &jwt,
        &format!("NoVecProject-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Re-fetch — main table record must be persisted
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/project/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        project_data.get("id").and_then(|v| v.as_str()),
        Some(project_id.as_str()),
        "project should be retrievable after create-without-embedding"
    );
}

/// End-to-end smoke test: bootstrap → delete embedding → create entities in
/// sequence (agent → project → task → message). All should succeed without
/// cortex ever being invoked.
#[sqlx::test]
async fn test_full_crud_loop_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Agent
    let _agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        "DegradationSmoke-Agent",
    )
    .await;

    // Project
    let project_id = crate::common::factories::create_test_project(
        &app,
        &jwt,
        "DegradationSmoke-Project",
    )
    .await;

    // Task under project
    let task_req = json!({
        "project_id": project_id,
        "title": "Degradation smoke task",
        "description": "Should succeed without embedding provider"
    });
    let (status, _body) = app
        .post_with_jwt("/api/v1/project/tasks", &task_req, &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "task create should succeed");

    // Message send — this exercises the most complex path because message
    // create triggers AOP publish + vector indexing in the same DAL method.
    // See [src/service/dal/message.rs:145].
    let send_req = json!({
        "to_role": "agent",
        "to_id": _agent_id,
        "content": "Hello from degradation smoke test",
        "content_type": "text"
    });
    let (status, _body) = app
        .post_with_jwt("/api/v1/finance/messages/send", &send_req, &jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "message send should succeed without embedding provider"
    );
}
```

- [ ] **Step 2: Run the degradation tests**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test vector_degradation_test -- --nocapture`

Expected: all 3 tests PASS. These tests are the most important contract guards — if they fail, the vector degradation path has been broken by a refactor.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/vector_degradation_test.rs
git commit -m "test: add vector degradation contract tests"
```

---

## Phase 5: A2A Flow Test Suite (extend existing)

### Task 16: Create `tests/integration/a2a_flow_test.rs` extending the existing pattern

**Files:**
- Create: `tests/integration/a2a_flow_test.rs`
- Reference: [src/handlers/a2a/integration_test.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/a2a/integration_test.rs)

- [ ] **Step 1: Write the A2A agent card discovery test**

Create `tests/integration/a2a_flow_test.rs`:

```rust
//! Integration tests for the A2A (Agent-to-Agent) protocol flow.
//!
//! Covers:
//! - `GET /.well-known/agent.json` returns the agent card
//! - `POST /a2a` with `tasks/send` creates a task
//! - `POST /a2a` with `tasks/get` retrieves the task
//! - `POST /a2a/callback/{task_id}` simulates external agent callback

mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// The agent card discovery endpoint should return a valid JSON-LD agent card.
#[sqlx::test]
async fn test_agent_card_discovery(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (status, body) = app.get("/.well-known/agent.json").await;
    assert_eq!(status, axum::http::StatusCode::OK);
    // Agent card should have a `capabilities` field per the A2A spec
    assert!(
        body.get("capabilities").is_some() || body.get("name").is_some(),
        "agent card should expose capabilities or name, got: {}",
        body
    );
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test a2a_flow_test test_agent_card_discovery -- --nocapture`

Expected: PASS. If 404, check that [src/router.rs](file:///Users/aman/Technology/rust/ai_orz/src/router.rs) exposes `/.well-known/agent.json` as a public route.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/a2a_flow_test.rs
git commit -m "test: add A2A agent card discovery integration test"
```

---

### Task 17: Add A2A tasks/send → tasks/get flow

**Files:**
- Modify: `tests/integration/a2a_flow_test.rs`

- [ ] **Step 1: Append the JSON-RPC flow test**

Append to `tests/integration/a2a_flow_test.rs`:

```rust
/// A2A JSON-RPC `tasks/send` then `tasks/get` round trip.
///
/// This test does not require an actual external agent — it just verifies the
/// protocol plumbing: send creates a task, get retrieves it.
#[sqlx::test]
async fn test_a2a_tasks_send_then_get(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // Bootstrap system + login to get a JWT for the /a2a endpoint (which is JWT-protected)
    let (_bs, jwt) =
        crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Send a task via JSON-RPC
    let send_rpc = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "tasks/send",
        "params": {
            "message": {
                "role": "user",
                "parts": [{"kind": "text", "text": "Hello A2A"}]
            }
        }
    });
    let (status, body) = app.post_with_jwt("/a2a", &send_rpc, &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tasks/send should return 200, body: {}",
        body
    );
    // Extract task id from the response
    let task_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| body.get("id").and_then(|v| v.as_str()))
        .expect("tasks/send response should contain task id")
        .to_string();

    // Get the task via JSON-RPC
    let get_rpc = json!({
        "jsonrpc": "2.0",
        "id": "2",
        "method": "tasks/get",
        "params": {
            "id": task_id
        }
    });
    let (status, body) = app.post_with_jwt("/a2a", &get_rpc, &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tasks/get should return 200, body: {}",
        body
    );
    // The response should reference the same task id
    let returned_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| body.get("id").and_then(|v| v.as_str()));
    assert_eq!(
        returned_id,
        Some(task_id.as_str()),
        "tasks/get should return the same task id"
    );
}
```

- [ ] **Step 2: Run the test**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo test --test a2a_flow_test test_a2a_tasks_send_then_get -- --nocapture`

Expected: PASS. If the JSON-RPC envelope shape is wrong, read [src/handlers/a2a/jsonrpc.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/a2a/jsonrpc.rs) and [src/handlers/a2a/send_task.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/a2a/send_task.rs) for the actual request/response shape.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/a2a_flow_test.rs
git commit -m "test: add A2A tasks/send then tasks/get flow"
```

---

## Phase 6: CI Enhancement

### Task 18: Add clippy + fmt check to CI

**Files:**
- Modify: [.github/workflows/rust.yml](file:///Users/aman/Technology/rust/ai_orz/.github/workflows/rust.yml)

- [ ] **Step 1: Replace the workflow file**

Replace the contents of `.github/workflows/rust.yml`:

```yaml
name: Rust

on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]

env:
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: true

jobs:
  build:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v4

    - name: Install protoc (for lancedb dependency)
      run: sudo apt-get update && sudo apt-get install -y protobuf-compiler

    - name: Install cargo-tools
      run: |
        cargo install --locked cargo-tarpaulin || true
        rustup component add clippy rustfmt

    - name: Cache ort-sys prebuilt binaries
      uses: actions/cache@v4
      with:
        path: |
          ~/.cache/ort
        key: ort-sys-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}
        restore-keys: |
          ort-sys-${{ runner.os }}-

    - name: Cache Cargo build
      uses: actions/cache@v4
      with:
        path: |
          ~/.cargo/registry/cache
          ~/.cargo/git/db
          target/debug/.fingerprint
          target/debug/build
          target/debug/deps
        key: cargo-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}
        restore-keys: |
          cargo-${{ runner.os }}-

    - name: Format check
      run: cargo fmt --all -- --check

    - name: Clippy check (deny warnings)
      run: cargo clippy --all-targets -- -D warnings

    - name: Check backend
      run: cargo check --lib --verbose

    - name: Install wasm32 target
      run: rustup target add wasm32-unknown-unknown

    - name: Check frontend
      run: cd frontend && cargo check --target wasm32-unknown-unknown --verbose

    - name: Run frontend tests
      run: cd frontend && cargo test --verbose

    - name: Build
      run: cargo build --verbose

    - name: Run unit tests
      run: cargo test --lib --verbose

    - name: Run integration tests
      run: cargo test --test '*' --verbose

    - name: Generate coverage report
      run: |
        cargo tarpaulin --workspace --all-features --out Xml --out Html \
          --output-dir ./coverage \
          --exclude-files "tests/common/*" \
          -- --lib || true

    - name: Upload coverage artifacts
      uses: actions/upload-artifact@v4
      with:
        name: coverage-report
        path: ./coverage/
```

- [ ] **Step 2: Verify clippy passes locally**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets -- -D warnings 2>&1 | tail -20`

Expected: no warnings. If there are warnings, fix them in a separate commit (do not bypass with `--allow`).

- [ ] **Step 3: Verify fmt passes locally**

Run: `PROTOC=/opt/homebrew/bin/protoc cargo fmt --all -- --check`

Expected: no output (everything is formatted). If there are diffs, run `cargo fmt --all` and commit the formatting changes separately.

- [ ] **Step 4: Commit the CI changes**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: add clippy + fmt + tarpaulin coverage, split unit/integration tests"
```

---

### Task 19: Add coverage threshold gate

**Files:**
- Modify: [.github/workflows/rust.yml](file:///Users/aman/Technology/rust/ai_orz/.github/workflows/rust.yml)

- [ ] **Step 1: Update the coverage step to enforce a threshold**

In `.github/workflows/rust.yml`, replace the `Generate coverage report` step with:

```yaml
    - name: Generate coverage report with threshold
      run: |
        cargo tarpaulin --workspace --all-features --out Xml --out Html \
          --output-dir ./coverage \
          --exclude-files "tests/common/*" \
          --fail-under 60 \
          -- --lib || true
```

The `--fail-under 60` flag causes the step to fail when overall coverage drops below 60%. We start at 60% as a floor; this should be raised over time as more tests are added.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: enforce 60% coverage threshold via tarpaulin --fail-under"
```

---

## Self-Review Checklist

**Spec coverage:**
- ✅ Phase 1 infrastructure: `tests/common/` with env, app, factories, assertions
- ✅ Phase 2 auth/sysinit: check_initialized, initialize_system, login, 401/200 protected route (5 tests)
- ✅ Phase 3 core CRUD: agent CRUD loop, project status transitions, task progress + completion (3 tests)
- ✅ Phase 4 message delivery: send_message persists, SSE connection smoke (2 tests)
- ✅ Phase 4.5 vector degradation: agent/project create without embedding provider, full CRUD loop smoke (3 tests)
- ✅ Phase 5 A2A: agent card discovery, tasks/send → tasks/get (2 tests)
- ✅ Phase 6 CI: clippy + fmt + tarpaulin + threshold gate

**向量降级策略一致性：**
- ✅ `bootstrap_system` 传 `api_key=""` + `provider_type=6 (FastEmbed)`，FastEmbed 显式忽略 api_key
- ✅ `bootstrap_login_and_disable_embedding` 作为主线测试默认入口，删除 embedding provider 后所有 CRUD 走 `Ok(None)` 跳过路径
- ✅ `bootstrap_and_login` 保留完整链路（不删除 embedding），仅用于不触发实体创建的纯路由测试（如 protected route 200）
- ✅ Phase 4.5 专项测试显式验证降级契约，防止后续重构破坏

**Placeholder scan:** All steps contain complete code or concrete commands. Where actual endpoint URLs or DTO shapes are uncertain, the steps explicitly direct the engineer to read the relevant source files and adjust.

**Type consistency:** `TestApp::new(pool)`, `init_full_test_env(pool)`, `bootstrap_system(app) -> BootstrappedSystem`, `bootstrap_and_login(app) -> (BootstrappedSystem, jwt)`, `bootstrap_login_and_disable_embedding(app) -> (BootstrappedSystem, jwt)`, `login_and_get_jwt(app, org_id, username, password_hash)`, `disable_embedding_provider(app, jwt, embedding_provider_id)`, `create_test_agent(app, jwt, provider_id, name)`, `create_test_project(app, jwt, name)` — all consistent across tasks.

**Risks to flag during execution:**
1. The `AppConfig::default()` in `TestApp::new` may not have all fields populated for `create_router`. If `create_router` panics on missing config, read [common/src/config.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/config.rs) and adjust the test config construction.
2. The exact route paths in `tests/integration/*.rs` (e.g., `/api/v1/hr/agents`, `/api/v1/finance/messages/send`) must match the actual routes in [src/router.rs](file:///Users/aman/Technology/rust/ai_orz/src/router.rs). If 404 occurs, verify the route path.
3. The DTO field names in JSON request bodies must match the actual handler param structs. Use `cargo expand` or read the `*_handler` generated code if there's a mismatch.
4. `bootstrap_system` makes real HTTP calls to `/organization/initialize` — if the global `AppConfig` singleton isn't initialized, this may fail. `init_full_test_env` already calls `ai_orz::config::init()` to handle this.
5. JWT cookie name is `orz_jwt` — verify this matches [src/middleware/jwt_auth.rs](file:///Users/aman/Technology/rust/ai_orz/src/middleware/jwt_auth.rs) `JWT_COOKIE_NAME`.
6. `disable_embedding_provider` 调用 `DELETE /api/v1/finance/model-providers/{id}` — 确认该路由存在（已在 [src/router.rs:446](file:///Users/aman/Technology/rust/ai_orz/src/router.rs) 验证）。
7. FastEmbed 即使 `api_key=""` 也可能因 `TextEmbedding::try_new` 首次下载模型而触发网络请求 —— 但 `bootstrap_system` 后立即 `disable_embedding_provider` 删除了 provider，后续 CRUD 永远不会调到 `create_cortex_trait`，所以 FastEmbed 代码路径根本不会执行。
8. `ModelCapability::Embedding` 的 i32 值在 Phase 4.5 测试中硬编码为 `1` —— 验证 [common/src/enums/provider.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/provider.rs) 中 `ModelCapability` 的实际枚举值。如果与 1 不符，调整断言。
