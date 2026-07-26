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
pub async fn bootstrap_login_and_disable_embedding(app: &TestApp) -> (BootstrappedSystem, String) {
    let (bs, jwt) = bootstrap_and_login(app).await;
    disable_embedding_provider(app, &jwt, &bs.embedding_provider_id).await;
    (bs, jwt)
}
