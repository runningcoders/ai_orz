//! 模型访问模式（access_mode）API 契约集成测试（方案 §2.2/§2.3）。
//!
//! 覆盖：创建落库（None=未配置）→ 详情/列表恒返回解析值（缺省 stream）；
//! Update None=不变更、Some=设置（无清除语义）；响应面取值为 serde snake_case
//!（"stream"/"non_stream"）。

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::api::CreateModelProviderRequest;
use ::common::enums::{ModelAccessMode, ModelCapability, ProviderType};
use sqlx::SqlitePool;

const PROVIDERS_PATH: &str = "/api/v1/finance/model-providers";

fn chat_req(name: &str, access_mode: Option<ModelAccessMode>) -> CreateModelProviderRequest {
    CreateModelProviderRequest {
        name: name.to_string(),
        provider_type: ProviderType::OpenAI,
        capability: ModelCapability::Agent,
        model_name: "gpt-4o".to_string(),
        api_key: "sk-test".to_string(),
        base_url: None,
        description: None,
        max_context_length: Some(128_000),
        recommended_context_length: None,
        access_mode,
    }
}

async fn create_provider(app: &TestApp, jwt: &str, req: CreateModelProviderRequest) -> String {
    let (status, body) = app.post_with_jwt(PROVIDERS_PATH, &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("created id")
        .to_string()
}

async fn get_detail(app: &TestApp, jwt: &str, id: &str) -> serde_json::Value {
    let (status, body) = app
        .get_with_jwt(&format!("{PROVIDERS_PATH}/{id}"), jwt)
        .await;
    crate::common::assert_api_ok(status, &body)
}

/// 创建时不传 access_mode → 详情/列表恒返回解析后的 stream（缺省=存量行为）
#[sqlx::test]
async fn test_create_without_access_mode_defaults_to_stream(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let id = create_provider(&app, &jwt, chat_req("Chat-Default", None)).await;
    let detail = get_detail(&app, &jwt, &id).await;
    assert_eq!(
        detail["access_mode"].as_str(),
        Some("stream"),
        "缺省必须解析为 stream，detail: {detail}"
    );

    let (status, body) = app.get_with_jwt(PROVIDERS_PATH, &jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    let item = data["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .find(|p| p["id"].as_str() == Some(id.as_str()))
        .expect("provider in list");
    assert_eq!(item["access_mode"].as_str(), Some("stream"));
}

/// 创建携带 non_stream → 落库并恒返回 non_stream（serde snake_case 契约值）
#[sqlx::test]
async fn test_create_with_non_stream_round_trips(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let id = create_provider(
        &app,
        &jwt,
        chat_req("Chat-NS", Some(ModelAccessMode::NonStream)),
    )
    .await;
    let detail = get_detail(&app, &jwt, &id).await;
    assert_eq!(detail["access_mode"].as_str(), Some("non_stream"));
}

/// Update 语义：None=不变更、Some=设置（无清除语义）（方案 §2.2）
#[sqlx::test]
async fn test_update_access_mode_none_means_no_change_and_some_sets(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let id = create_provider(
        &app,
        &jwt,
        chat_req("Chat-Upd", Some(ModelAccessMode::NonStream)),
    )
    .await;
    assert_eq!(
        get_detail(&app, &jwt, &id).await["access_mode"].as_str(),
        Some("non_stream")
    );

    // 不带 access_mode 的更新 → 不变更（None=不变更）
    let (status, body) = app
        .put_with_jwt(
            &format!("{PROVIDERS_PATH}/{id}"),
            &serde_json::json!({ "id": id.clone() }),
            &jwt,
        )
        .await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        get_detail(&app, &jwt, &id).await["access_mode"].as_str(),
        Some("non_stream"),
        "None 不得清除已有 access_mode"
    );

    // 显式切回 stream → 设置生效
    let (status, body) = app
        .put_with_jwt(
            &format!("{PROVIDERS_PATH}/{id}"),
            &serde_json::json!({ "id": id, "access_mode": "stream" }),
            &jwt,
        )
        .await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        get_detail(&app, &jwt, &id).await["access_mode"].as_str(),
        Some("stream")
    );
}
