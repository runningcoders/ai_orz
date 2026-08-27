//! User/auth test factories.
//!
//! Provides helpers to:
//! 1. Bootstrap a system (org + admin user + chat model provider) via the real
//!    `/organization/initialize` endpoint. **先查后建**：如果 Local 组织已存在，
//!    直接复用已有数据（从 service 层读取）；否则才走 HTTP 初始化流程。
//!    `embedding_model` 是 `None` — 测试环境永不配置 embedding provider。
//! 2. Login as the admin user via the real `/organization/auth/login` endpoint
//!    and return a JWT token.
//!
//! 设计锚点：一台设备只能有一个 Local 组织（scope=0）。集成测试共享同一
//! SQLite DB，第一个 bootstrap 创建 Local 组织后，后续测试必须复用它，
//! 不能再调 `/initialize` 创建新的 Local 组织——否则会被 handler 的
//! "系统已初始化"检查拦截。

use crate::common::app::TestApp;
use ai_orz::service::dao::model_provider::ModelProviderQuery;
use ai_orz::service::dao::organization::OrganizationQuery;
use ai_orz::service::dao::user::UserQuery;
use ai_orz::service::domain::{finance, organization};
use common::api::{
    InitializeSystemRequest, LoginRequest, ModelProviderInitConfig, PaginationParams,
};
use common::enums::{ModelCapability, OrganizationScope, UserRole};

/// 全局互斥锁：所有集成测试共享同一个全局 DB，`bootstrap_system` 必须串行执行，
/// 避免并行 init 任务导致 `sync_builtin_tools` 竞争（UNIQUE constraint）和
/// 预置技能固定 ID 被并发覆盖（author_id 错乱）。
static BOOTSTRAP_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Bootstrap result — contains everything tests need to make authenticated calls.
#[derive(Debug, Clone)]
#[allow(dead_code)] // 公共测试 API 字段，保留供未来测试使用
pub struct BootstrappedSystem {
    pub organization_id: String,
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub chat_provider_id: String,
    /// None 表示测试环境未配置 embedding provider（默认情况）。
    /// 所有实体创建走 `Ok(None)` 向量降级路径，不触发 cortex/FastEmbed。
    pub embedding_provider_id: Option<String>,
}

/// 轮询初始化进度直到完成或失败（供各初始化变体复用）
pub async fn poll_initialize_progress(app: &TestApp, task_id: &str) -> serde_json::Value {
    loop {
        let (status, body) = app
            .get(&format!(
                "/api/v1/organization/initialize/progress?task_id={}",
                task_id
            ))
            .await;
        let data = crate::common::assert_api_ok(status, &body);
        let status_str = data
            .get("status")
            .and_then(|v| v.as_str())
            .expect("missing status in progress response");

        match status_str {
            "completed" => {
                return data
                    .get("result")
                    .expect("missing result in completed progress")
                    .clone();
            }
            "failed" => {
                let error = data
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                panic!("系统初始化失败: {}", error);
            }
            _ => {
                // pending 或 running，等待后重试
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// 尝试从 service 层直接复用已存在的 Local 组织 + admin 用户 + chat provider。
///
/// 返回 `Some` 表示复用成功（Local 组织存在 + SuperAdmin 用户存在）；
/// 返回 `None` 表示需要走 HTTP initialize 创建新系统。
async fn try_reuse_existing() -> Option<BootstrappedSystem> {
    let ctx = ai_orz::pkg::RequestContext::from_storage(
        "test-bootstrap-reuse",
        ai_orz::pkg::storage::get().clone(),
    );

    // 1. 查 Local 组织
    let local_orgs = organization::domain()
        .organization_manage()
        .query(
            ctx.clone(),
            OrganizationQuery {
                scope: Some(OrganizationScope::Local),
                ..Default::default()
            },
        )
        .await
        .ok()?;
    let org = local_orgs.into_iter().next()?;
    let org_id = org.id;

    // 2. 查该组织的 SuperAdmin 用户
    let users = organization::domain()
        .user_manage()
        .query(
            ctx.clone(),
            UserQuery {
                organization_id: Some(org_id.clone()),
                pagination: PaginationParams {
                    limit: Some(50),
                    offset: None,
                },
            },
        )
        .await
        .ok()?;
    let admin = users
        .items
        .into_iter()
        .find(|u| u.role == UserRole::SuperAdmin)?;
    let user_id = admin.id.clone();
    let username = admin.username.clone();
    let password_hash = admin.password_hash.clone();

    // 3. 查 chat model provider（Agent capability，由该 admin 创建）
    let providers = finance::domain()
        .model_provider_manage()
        .query(
            ctx.clone(),
            ModelProviderQuery {
                capability: Some(ModelCapability::Agent),
                pagination: PaginationParams {
                    limit: Some(50),
                    offset: None,
                },
                ..Default::default()
            },
        )
        .await
        .ok()?;
    let chat_provider = providers
        .items
        .into_iter()
        .find(|p| p.po.created_by == user_id)?;
    let chat_provider_id = chat_provider.po.id.clone();

    Some(BootstrappedSystem {
        organization_id: org_id,
        user_id,
        username,
        password_hash,
        chat_provider_id,
        embedding_provider_id: None, // bootstrap_system 默认不创建 embedding
    })
}

/// Bootstrap the system with one org, one admin, and one chat model provider.
///
/// **先查后建**：如果 Local 组织 + admin + chat provider 已存在，直接从 service
/// 层读取复用；否则走 HTTP `/initialize` 创建。
///
/// **向量降级关键**：`embedding_model: None` —— 不创建 embedding provider，
/// `get_default_embedding_provider` 直接返回 `Ok(None)`，所有 DAL 的
/// `embed_entity` 被跳过，永远不会触发 FastEmbed 模型加载。
pub async fn bootstrap_system(app: &TestApp) -> BootstrappedSystem {
    // 串行化：所有测试共享同一全局 DB，避免并行 init 竞争
    let _guard = BOOTSTRAP_MUTEX.lock().await;

    // 先查：已有 Local 组织 → 直接复用（避免触发 handler 的"系统已初始化"拦截）
    if let Some(bs) = try_reuse_existing().await {
        return bs;
    }

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
        chat_model: Some(ModelProviderInitConfig {
            name: "Test Chat Provider".to_string(),
            provider_type: 0, // OpenAI
            model_name: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            description: Some("test chat model".to_string()),
        }),
        embedding_model: None,
    };

    let (status, body) = app.post("/api/v1/organization/initialize", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    let task_id = data
        .get("task_id")
        .and_then(|v| v.as_str())
        .expect("missing task_id in initialize response")
        .to_string();

    // 轮询进度直到完成
    let result = poll_initialize_progress(app, &task_id).await;

    let org_id = result
        .get("organization_id")
        .and_then(|v| v.as_str())
        .expect("missing organization_id in progress result")
        .to_string();
    let user_id = result
        .get("user_id")
        .and_then(|v| v.as_str())
        .expect("missing user_id in progress result")
        .to_string();
    let chat_provider_id = result
        .get("chat_provider_id")
        .and_then(|v| v.as_str())
        .expect("missing chat_provider_id in progress result")
        .to_string();
    let embedding_provider_id = result
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

/// 最小初始化变体：跳过对话模型（`chat_model: None`），embedding 可选传入。
///
/// 返回 `(原始结果 JSON, admin 用户名, admin 密码哈希)`：
/// - 不组装 `BootstrappedSystem` —— 跳过 chat 时 `chat_provider_id` 为 null，
///   其 `String` 字段会 parse 失败，调用方直接按 JSON 断言；
/// - 附带凭证供用例验证最小初始化后管理员「可登录」。
#[allow(dead_code)] // 公共测试 API，保留供未来测试使用
pub async fn bootstrap_system_minimal(
    app: &TestApp,
    embedding_model: Option<ModelProviderInitConfig>,
) -> (serde_json::Value, String, String) {
    // 串行化：与 bootstrap_system 共享同一把锁，避免并行 init 竞争
    let _guard = BOOTSTRAP_MUTEX.lock().await;

    let username = format!("min-admin-{}", uuid::Uuid::now_v7());
    let password_hash = format!("hash-{}", uuid::Uuid::now_v7());
    let org_name = format!("MinOrg-{}", uuid::Uuid::now_v7());

    let req = InitializeSystemRequest {
        organization_name: org_name,
        admin_username: username.clone(),
        admin_password_hash: password_hash.clone(),
        description: None,
        admin_display_name: None,
        admin_email: None,
        chat_model: None,
        embedding_model,
    };

    let (status, body) = app.post("/api/v1/organization/initialize", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    let task_id = data
        .get("task_id")
        .and_then(|v| v.as_str())
        .expect("missing task_id in initialize response")
        .to_string();

    let result = poll_initialize_progress(app, &task_id).await;
    (result, username, password_hash)
}

/// Login as the given user via the real `/organization/auth/login` endpoint.
///
/// Returns the JWT token. Tests should pass this to `TestApp::get_with_jwt` etc.
#[allow(dead_code)] // 公共测试 API，保留供未来测试使用
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
#[allow(dead_code)] // 公共测试 API，保留供未来测试使用
pub async fn bootstrap_and_login(app: &TestApp) -> (BootstrappedSystem, String) {
    let bs = bootstrap_system(app).await;
    let jwt = login_and_get_jwt(app, &bs.organization_id, &bs.username, &bs.password_hash).await;
    (bs, jwt)
}
