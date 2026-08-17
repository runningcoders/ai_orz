//! Embedding provider 生命周期集成测试。
//!
//! 策略（创建不阻塞 + 启用时切换）：
//! - 首个 Embedding 创建 → 直接启用（Normal），并注册全量向量重建（后补场景）
//! - 已有启用的 Embedding 时再创建 → 成功但落库为未启用（Disabled=2），不注册重建
//! - 启用 Disabled 的 Embedding → 409 switch_required → 走 switch 完成切换确认
//! - 编辑 Normal 的 Embedding 配置（model_name/api_key/base_url）→ 注册重建；
//!   编辑 Disabled 的 → 不注册（启用切换时 switch 全量重建兜底）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::api::CreateModelProviderRequest;
use ::common::enums::{ModelCapability, ProviderType};
use sqlx::SqlitePool;

/// Embedding 状态敏感用例互斥锁：所有集成测试共享同一个全局 DB 的
/// 「已启用 Embedding Provider」状态，本文件用例必须串行执行 + 用例开始前
/// 清理启用态，才能拿到确定性的「首个/第二个」创建场景。
static EMBEDDING_STATE_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn embedding_req(name: &str) -> CreateModelProviderRequest {
    CreateModelProviderRequest {
        name: name.to_string(),
        provider_type: ProviderType::FastEmbed,
        capability: ModelCapability::Embedding,
        model_name: "BAAI/bge-small-en-v1.5".to_string(),
        api_key: String::new(),
        base_url: None,
        description: None,
        max_context_length: None,
        recommended_context_length: None,
    }
}

const PROVIDERS_PATH: &str = "/api/v1/finance/model-providers";

/// 清理全局启用态：软删所有已启用的 Embedding provider，使后续断言拿到
/// 「无任何启用者」的初始态（其他测试文件不创建 embedding provider）。
async fn ensure_no_enabled_embedding(app: &TestApp, jwt: &str) {
    let (status, body) = app.get_with_jwt(PROVIDERS_PATH, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    for p in data["providers"].as_array().expect("providers array") {
        if p["capability"].as_str() == Some("Embedding") && p["status"].as_i64() == Some(1) {
            let id = p["id"].as_str().expect("provider id").to_string();
            let _ = app
                .delete_with_jwt(&format!("{}/{}", PROVIDERS_PATH, id), jwt)
                .await;
        }
    }
}

/// 已有启用的 Embedding 时，创建第二个成功但为未启用状态（Disabled=2）
#[sqlx::test]
async fn test_create_second_embedding_lands_disabled(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    // 首个 → 启用
    let (status, _) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-A"), &jwt)
        .await;
    assert_eq!(
        status, 200,
        "first embedding provider should be created enabled"
    );

    // 第二个 → 成功但 Disabled
    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-B"), &jwt)
        .await;
    assert_eq!(
        status, 200,
        "second embedding provider should be created (not rejected)"
    );
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(2),
        "second embedding provider should land as Disabled(2), got: {:?}",
        data.get("status")
    );

    // 列表中 A 仍唯一启用（capability 序列化为字符串 "Embedding"）
    let (status, body) = app.get_with_jwt(PROVIDERS_PATH, &jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    let enabled_count = data["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .filter(|p| {
            p["capability"].as_str() == Some("Embedding") && p["status"].as_i64() == Some(1)
        })
        .count();
    assert_eq!(enabled_count, 1, "exactly one enabled embedding provider");
}

/// 启用 Disabled 的 Embedding → 409 switch_required（切换确认由既有 switch 链路完成）
#[sqlx::test]
async fn test_enable_disabled_embedding_requires_switch_confirm(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    let _ = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-A"), &jwt)
        .await;
    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-B"), &jwt)
        .await;
    let b_id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("created id")
        .to_string();

    // 直接启用 B → 409 + switch_required（前端据此弹确认 modal）
    let update = serde_json::json!({ "id": b_id, "status": 1 });
    let (status, body) = app
        .put_with_jwt(&format!("{}/{}", PROVIDERS_PATH, b_id), &update, &jwt)
        .await;
    assert_eq!(status, 409);
    assert!(
        body.to_string()
            .contains("embedding_provider_switch_required"),
        "expected switch_required error, got: {}",
        body
    );
}

/// 后补场景：初始化未配向量模型，事后创建首个 Embedding（落库 Normal）→ 携带 rebuild_task_id
#[sqlx::test]
async fn test_create_first_embedding_provider_triggers_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-Late"), &jwt)
        .await;
    assert_eq!(status, 200);
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(1),
        "first embedding lands Normal"
    );
    assert!(
        data.get("rebuild_task_id")
            .and_then(|v| v.as_str())
            .is_some(),
        "first embedding creation should register rebuild task"
    );
}

/// 已有启用者时创建第二个（Disabled）→ 不携带 rebuild_task_id（重建推迟到切换时）
#[sqlx::test]
async fn test_create_disabled_embedding_no_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    let _ = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-A"), &jwt)
        .await;
    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-B"), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(2),
        "second embedding lands Disabled"
    );
    assert!(
        data.get("rebuild_task_id").is_none(),
        "disabled embedding creation must NOT register rebuild (deferred to switch)"
    );
}

/// 编辑使用中（Normal）embedding 的 model_name → 触发重建
#[sqlx::test]
async fn test_update_enabled_embedding_model_triggers_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-Edit"), &jwt)
        .await;
    let id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("id")
        .to_string();

    let update = serde_json::json!({ "id": id, "model_name": "BAAI/bge-m3" });
    let (status, body) = app
        .put_with_jwt(&format!("{}/{}", PROVIDERS_PATH, id), &update, &jwt)
        .await;
    assert_eq!(status, 200);
    assert!(
        crate::common::assert_api_ok(status, &body)
            .get("rebuild_task_id")
            .and_then(|v| v.as_str())
            .is_some(),
        "model_name change on enabled embedding should trigger rebuild"
    );
}

/// 编辑未启用（Disabled）embedding 的 model_name → 不触发重建
#[sqlx::test]
async fn test_update_disabled_embedding_no_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let _guard = EMBEDDING_STATE_MUTEX.lock().await;
    ensure_no_enabled_embedding(&app, &jwt).await;

    let _ = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-A"), &jwt)
        .await;
    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &embedding_req("Embedding-B"), &jwt)
        .await;
    let b_id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("id")
        .to_string();

    let update = serde_json::json!({ "id": b_id, "model_name": "BAAI/bge-m3" });
    let (status, body) = app
        .put_with_jwt(&format!("{}/{}", PROVIDERS_PATH, b_id), &update, &jwt)
        .await;
    assert_eq!(status, 200);
    assert!(
        crate::common::assert_api_ok(status, &body)
            .get("rebuild_task_id")
            .is_none(),
        "editing disabled embedding must NOT trigger rebuild"
    );
}
