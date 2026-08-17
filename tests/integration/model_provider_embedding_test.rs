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

/// 已有启用的 Embedding 时，创建第二个成功但为未启用状态（Disabled=2）
#[sqlx::test]
async fn test_create_second_embedding_lands_disabled(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

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
