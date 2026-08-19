//! Integration tests for the Lark message channel lifecycle.
//!
//! Covers（无真实飞书网络，断言失败路径与响应结构）：
//! - 为用户写入 LarkApp 凭证（user_credentials 表）→ 创建 Lark 渠道（凭证引用）
//!   → 详情回显引用 ID/凭证名且 secret 绝不出现
//! - 创建 Lark 渠道缺少凭证引用 → 400 校验错误
//! - 连接测试 → 假凭证/无网络场景返回结构化失败（success=false + error）
//! - 状态变更（禁用）→ 详情状态同步 → 删除
//!
//! 路由：`/api/v1/finance/message-channels`（见 `src/router.rs::finance_routes`）

#[path = "../common/mod.rs"]
mod common;

use ::common::models::CredentialDetail;
use serde_json::json;
use sqlx::SqlitePool;

/// 直接写 user_credentials 表（测试期最短路径，kind/visibility 为 snake_case 字符串）
///
/// 注意：handler 链路走全局 storage 连接池（init_full_test_env 初始化），
/// 与 sqlx::test 注入的隔离池不是同一个库，故 seed 必须写全局池。
async fn seed_user_lark_credential(user_id: &str, credential_id: &str, app_secret: &str) {
    let detail = CredentialDetail::LarkApp {
        app_id: "cli_integration_test".to_string(),
        // 明文直通兼容：解密路径对未加密值原样返回
        app_secret: app_secret.to_string(),
        encrypt_key: None,
        verification_token: None,
    };
    let now = ::common::constants::utils::current_timestamp_ms();
    sqlx::query(
        r#"
INSERT INTO user_credentials
    (id, org_id, user_id, kind, name, detail, visibility, is_default, status,
     created_by, modified_by, created_at, updated_at)
VALUES (?, 'org-1', ?, 'lark_app', '集成测试凭证', ?, 'private', 0, 1, 'system', 'system', ?, ?)
        "#,
    )
    .bind(credential_id)
    .bind(user_id)
    .bind(serde_json::to_string(&detail).expect("serialize credential detail"))
    .bind(now)
    .bind(now)
    .execute(ai_orz::pkg::storage::get().sqlite_pool())
    .await
    .expect("seed user_credentials failed");
}

/// Lark 渠道全链路：创建（凭证引用）→ 脱敏断言 → 连接测试（结构化失败）→ 禁用 → 删除
#[sqlx::test]
async fn test_lark_channel_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let app_secret = format!("fake-lark-secret-{}", uuid::Uuid::now_v7());
    seed_user_lark_credential(&bs.user_id, "cred-lifecycle", &app_secret).await;

    // 1. 创建 Lark 渠道（引用已绑定凭证）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/message-channels",
            &json!({
                "channel_type": "Lark",
                "channel_name": "集成测试飞书渠道",
                "lark_credential_id": "cred-lifecycle",
                "lark_identity_mode": "bot",
                "lark_open_id": "ou_integration",
                "lark_user_name": "集成测试用户",
                "lark_listen_inbound": false
            }),
            &jwt,
        )
        .await;
    let detail = crate::common::assert_api_ok(status, &body);

    let channel_id = detail
        .get("id")
        .and_then(|v| v.as_str())
        .expect("channel detail missing id")
        .to_string();
    assert_eq!(
        detail.get("lark_credential_id").and_then(|v| v.as_str()),
        Some("cred-lifecycle")
    );
    assert_eq!(
        detail.get("lark_credential_name").and_then(|v| v.as_str()),
        Some("集成测试凭证")
    );
    assert_eq!(
        detail.get("lark_identity_mode").and_then(|v| v.as_str()),
        Some("bot")
    );
    assert_eq!(
        detail.get("lark_listen_inbound").and_then(|v| v.as_bool()),
        Some(false)
    );
    // 守护：响应全链路绝不回显 secret 明文
    assert!(
        !body.to_string().contains(&app_secret),
        "channel response must never echo app_secret"
    );

    // 2. 连接测试：假凭证（或无网络）应返回结构化失败而非 5xx
    let (status, body) = app
        .post_with_jwt(
            &format!("/api/v1/finance/message-channels/{}/test", channel_id),
            &json!({ "id": channel_id }),
            &jwt,
        )
        .await;
    let test_result = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        test_result.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "fake credentials should fail the connection test"
    );
    assert!(
        test_result
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "failed connection test should carry error message"
    );

    // 3. 禁用渠道
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/finance/message-channels/{}/status", channel_id),
            &json!({ "id": channel_id, "status": "Disabled" }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);

    // 4. 详情状态同步为禁用（且仍不回显 secret）
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/finance/message-channels/{}", channel_id),
            &jwt,
        )
        .await;
    let detail = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        detail.get("status").and_then(|v| v.as_str()),
        Some("Disabled")
    );
    assert!(!body.to_string().contains(&app_secret));

    // 5. 删除渠道
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/message-channels/{}", channel_id),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
}

/// Lark 渠道创建缺少/无效凭证引用 → 校验错误（400/InvalidRequest）
#[sqlx::test]
async fn test_create_lark_channel_requires_credential_ref(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 缺凭证引用
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/message-channels",
            &json!({
                "channel_type": "Lark",
                "channel_name": "缺凭证渠道"
            }),
            &jwt,
        )
        .await;
    assert!(status.is_client_error(), "expected 4xx, got {}", status);
    assert!(
        body.to_string().contains("凭证"),
        "error should mention missing credential ref: {}",
        body
    );

    // 引用不存在的凭证
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/message-channels",
            &json!({
                "channel_type": "Lark",
                "channel_name": "悬空引用渠道",
                "lark_credential_id": "cred-does-not-exist"
            }),
            &jwt,
        )
        .await;
    assert!(status.is_client_error(), "expected 4xx, got {}", status);
    assert!(
        body.to_string().contains("不存在"),
        "error should mention dangling credential ref: {}",
        body
    );
}
