//! User/auth test factories.
//!
//! Provides helpers to:
//! 1. Bootstrap a system (org + admin user + chat model provider) via the real
//!    `/organization/initialize` endpoint. `embedding_model` is `None` so the
//!    test environment never configures an embedding provider — all entity
//!    creates take the `Ok(None)` vector-degradation path (no cortex calls).
//! 2. Login as the admin user via the real `/organization/auth/login` endpoint
//!    and return a JWT token.

use crate::common::app::TestApp;
use common::api::{InitializeSystemRequest, LoginRequest, ModelProviderInitConfig};

/// Bootstrap result — contains everything tests need to make authenticated calls.
#[derive(Debug, Clone)]
pub struct BootstrappedSystem {
    pub organization_id: String,
    #[allow(dead_code)] // 公共测试 API 字段，保留供未来测试使用
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub chat_provider_id: String,
    /// None 表示测试环境未配置 embedding provider（默认情况）。
    /// 所有实体创建走 `Ok(None)` 向量降级路径，不触发 cortex/FastEmbed。
    #[allow(dead_code)] // 公共测试 API 字段，保留供未来测试使用
    pub embedding_provider_id: Option<String>,
}

/// Bootstrap the system with one org, one admin, and one chat model provider.
///
/// **向量降级关键**：`embedding_model: None` —— 不创建 embedding provider，
/// `get_default_embedding_provider` 直接返回 `Ok(None)`，所有 DAL 的
/// `embed_entity` 被跳过，永远不会触发 FastEmbed 模型加载。
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
        embedding_model: None,
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
        .map(|s| s.to_string());
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

/// Convenience: bootstrap system + login, returning
/// `(BootstrappedSystem, jwt_token)`.
///
/// This is the **default entry point** for most integration tests. Because
/// `bootstrap_system` passes `embedding_model: None`, no embedding provider is
/// ever created — all entity creates take the vector-degradation path with no
/// cortex calls and no FastEmbed model downloads, keeping tests fast and
/// CI-stable.
pub async fn bootstrap_and_login(app: &TestApp) -> (BootstrappedSystem, String) {
    let bs = bootstrap_system(app).await;
    let jwt = login_and_get_jwt(app, &bs.organization_id, &bs.username, &bs.password_hash).await;
    (bs, jwt)
}
