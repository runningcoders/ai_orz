//! `POST /model-providers/{id}/test` 参数契约回归
//!
//! 事故原型：前端 `test_model_provider_connection` 只发 `{}`，而旧宏实现要求带
//! `#[param(source = "path")]` 的 `id` 也出现在 JSON body 里 ⇒ `422 missing field \`id\``
//!（`Json<DTO>` 在进入 handler 之前就拒了请求，事后那句 `params.id = id` 覆盖永远执行不到）。
//!
//! 现契约：path 字段来自 URL，body 可整体省略；`prompt` 省略则服务端回落默认 "Hello!"。
//! 连通性失败一律以 `success=false` + `error` 返回，**不用 HTTP 错误码表达**（handler 注释：
//! "never fails on upstream errors"），所以这里的判据是「不是 422」而不是「状态码 200」。

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::api::{CreateModelProviderRequest, UpdateModelProviderRequest};
use ::common::enums::{ModelCapability, ProviderType};
use sqlx::SqlitePool;

const PROVIDERS_PATH: &str = "/api/v1/finance/model-providers";

/// base_url 指向本机未监听端口：连通性测试必然失败，但不产生任何外网流量
fn unreachable_provider(name: &str) -> CreateModelProviderRequest {
    CreateModelProviderRequest {
        name: name.to_string(),
        provider_type: ProviderType::OpenAI,
        capability: ModelCapability::Agent,
        model_name: "gpt-4o".to_string(),
        api_key: "sk-test".to_string(),
        base_url: Some("http://127.0.0.1:1/v1".to_string()),
        description: None,
        max_context_length: Some(128_000),
        recommended_context_length: None,
        access_mode: None,
        supports_vision: None,
        // 本文件专注连通性测试契约，协议口味走缺省
        api_flavor: None,
    }
}

async fn create_provider(app: &TestApp, jwt: &str, name: &str) -> String {
    let (status, body) = app
        .post_with_jwt(PROVIDERS_PATH, &unreachable_provider(name), jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("created id")
        .to_string()
}

/// 空 body 不再是 missing field；请求应进入业务路径（连不通 ⇒ success=false）
#[sqlx::test]
async fn test_connection_with_empty_body_reaches_handler(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let id = create_provider(&app, &jwt, "Unreachable-Empty").await;

    let (status, body) = app
        .post_with_jwt(
            &format!("{PROVIDERS_PATH}/{id}/test"),
            &serde_json::json!({}),
            &jwt,
        )
        .await;

    assert!(
        status != axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "空 body 不得再触发 422 missing field `id`，实际 status={status}, body={body}"
    );
    assert!(
        status.is_success(),
        "应进入业务路径并返回成功包装，实际 status={status}, body={body}"
    );
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data["success"], false, "不可达 provider 应 success=false");
    assert!(
        data["error"].is_string(),
        "失败应带 error 文案，实际: {data}"
    );
}

/// body 只带 prompt、不带 id ⇒ 同样走业务路径（path 提供 id）
#[sqlx::test]
async fn test_connection_with_prompt_only_body_uses_path_id(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let id = create_provider(&app, &jwt, "Unreachable-Prompt").await;

    let (status, body) = app
        .post_with_jwt(
            &format!("{PROVIDERS_PATH}/{id}/test"),
            &serde_json::json!({ "prompt": "ping" }),
            &jwt,
        )
        .await;

    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data["success"], false, "不可达 provider 应 success=false");
}

/// path 值必须覆盖 body 里的同名 id：body 伪造 id 时，命中的仍是 path 指向的 provider
#[sqlx::test]
async fn test_path_id_overrides_body_id(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let real_id = create_provider(&app, &jwt, "Override-Target").await;

    // body 里的 id 是伪造的；若被采信 ⇒ Provider 不存在 ⇒ 404
    let forged = UpdateModelProviderRequest {
        id: "forged-provider-id".to_string(),
        name: Some("Renamed-By-Path-Id".to_string()),
        ..Default::default()
    };
    let (status, body) = app
        .put_with_jwt(&format!("{PROVIDERS_PATH}/{real_id}"), &forged, &jwt)
        .await;
    crate::common::assert_api_ok(status, &body);

    // 校验：真实 provider 被改名了（说明 path id 生效）
    let (status, body) = app
        .get_with_jwt(&format!("{PROVIDERS_PATH}/{real_id}"), &jwt)
        .await;
    let detail = crate::common::assert_api_ok(status, &body);
    assert_eq!(detail["name"], "Renamed-By-Path-Id");
}
