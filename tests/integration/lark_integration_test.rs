//! Integration tests for the user-level Lark integration endpoints.
//!
//! Covers（CI 无 lark-cli 二进制，开发机有二进制但测试用户 HOME 无 lark-cli config）：
//! - auth/bind 各端点在无 CLI 配置时返回引导性错误 JSON（4xx，非 5xx，不含 secret 明文）
//! - 凭证-渠道引用生命周期：建凭证 → 建渠道引用 → 聚合快照反查 → 删凭证被引用拦截
//!   → 删渠道留凭证 → 改凭证 → 删凭证成功
//!
//! 路由：`/api/v1/finance/identity/lark/`（见 `src/router.rs::finance_routes`）

#[path = "../common/mod.rs"]
mod common;

use serde_json::json;
use sqlx::SqlitePool;

/// 无 CLI 配置/二进制时：auth 与 bind 端点均返回引导性 4xx JSON（非 5xx）
#[sqlx::test]
async fn test_lark_integration_endpoints_guide_errors(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // auth/start：HOME 下无 lark-cli config → 引导错误
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/lark/auth/start",
            &json!({ "domains": [] }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "auth/start should return 4xx guidance, got {}",
        status
    );
    assert!(
        body.get("message")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "guidance error should carry message: {}",
        body
    );

    // auth/status：降级为 200 + logged_in=false + 引导 hint（不抛 5xx）
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/lark/auth/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("logged_in").and_then(|v| v.as_bool()),
        Some(false),
        "auth/status should degrade to logged_in=false: {}",
        body
    );
    assert!(
        data.get("hint")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "degraded auth/status should carry guidance hint: {}",
        body
    );

    // bind/start：无二进制/无 HOME 时为引导错误（4xx）；开发机有 lark-cli 时会真实发起
    // 会话（200）——两种结果均合法，断言绝不 5xx，且真实发起时立即取消避免遗留进程
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/identity/lark/bind/start", &json!({}), &jwt)
        .await;
    assert!(
        status.is_success() || status.is_client_error(),
        "bind/start must never 5xx, got {}",
        status
    );
    if status.is_success() {
        let session_id = body
            .get("data")
            .and_then(|d| d.get("session_id"))
            .and_then(|v| v.as_str())
            .expect("bind session_id")
            .to_string();
        let (status, body) = app
            .post_with_jwt(
                "/api/v1/finance/identity/lark/bind/cancel",
                &json!({ "session_id": session_id }),
                &jwt,
            )
            .await;
        let data = crate::common::assert_api_ok(status, &body);
        assert_eq!(data.get("success").and_then(|v| v.as_bool()), Some(true));
    }

    // 守护：引导错误响应中绝不出现 secret 字样对应的明文泄漏结构
    assert!(
        !body.to_string().contains("app_secret\":\""),
        "guidance response must never echo secrets: {}",
        body
    );

    // bind/status：未知会话 → NotFound
    let (status, _body) = app
        .get_with_jwt(
            "/api/v1/finance/identity/lark/bind/status?session_id=no-such-session",
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "bind/status unknown session should return 4xx, got {}",
        status
    );
}

/// 聚合快照端点：无凭证时返回空 credentials + user_auth 降级（不 5xx）
#[sqlx::test]
async fn test_lark_integration_status_empty(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, _admin_jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 共享 DB 下复用的 SuperAdmin 可能残留其他用例的凭证，
    // 用邀请码注册一个真正全新的成员作为空状态主体
    let (member_jwt, _member_id, _member_org) =
        crate::common::factories::register_fresh_member(&app).await;

    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/lark/status", &member_jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert!(credentials.is_empty(), "fresh user has no credentials");
    assert_eq!(
        data.get("user_auth")
            .and_then(|v| v.get("logged_in"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "user_auth should degrade to logged_in=false instead of 5xx"
    );
}

/// 凭证-渠道引用生命周期（建/引/拦/删/改全链路）
#[sqlx::test]
async fn test_credential_channel_reference_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭证与渠道持有者改为全新注册成员：结尾「快照清空」断言只看本用例
    // 写入的数据，不受 sibling 用例向复用管理员累积数据的影响。
    let (jwt, _member_id, _member_org) =
        crate::common::factories::register_fresh_member(&app).await;

    let app_secret = format!("fake-lark-secret-{}", uuid::Uuid::now_v7());

    // 1. 创建凭证
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/lark/credentials",
            &json!({
                "name": "生命周期凭证",
                "app_id": "cli_lifecycle",
                "app_secret": app_secret
            }),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credential_id = data
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id")
        .to_string();
    assert!(
        !body.to_string().contains(&app_secret),
        "create credential response must never echo app_secret"
    );

    // 2. 聚合快照：凭证在列、渠道为空
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/lark/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let cred = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find(|c| c["credential_id"] == credential_id))
        .expect("credential snapshot present");
    assert_eq!(cred["app_id"], "cli_lifecycle");
    assert!(
        cred["channels"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "no channel references yet"
    );
    assert!(
        !body.to_string().contains(&app_secret),
        "status snapshot must never echo app_secret"
    );

    // 3. 创建渠道引用该凭证
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/message-channels",
            &json!({
                "channel_type": "Lark",
                "channel_name": "生命周期渠道",
                "config": {
                    "lark": {
                        "credential_id": credential_id,
                        "listen_inbound": false
                    }
                }
            }),
            &jwt,
        )
        .await;
    let channel = crate::common::assert_api_ok(status, &body);
    let channel_id = channel["id"].as_str().expect("channel id").to_string();

    // 4. 聚合快照反查渠道
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/lark/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let cred = data["credentials"]
        .as_array()
        .and_then(|arr| arr.iter().find(|c| c["credential_id"] == credential_id))
        .expect("credential snapshot present");
    let channels = cred["channels"].as_array().expect("channels array");
    assert_eq!(channels.len(), 1, "channel reference visible in snapshot");
    assert_eq!(channels[0]["channel_id"], channel_id);

    // 5. 删除凭证被引用拦截（Conflict）
    let (status, body) = app
        .delete_with_jwt(
            &format!(
                "/api/v1/finance/identity/lark/credentials/{}",
                credential_id
            ),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "delete referenced credential should be rejected, got {}",
        status
    );
    let _ = body;

    // 6. 删渠道留凭证
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/message-channels/{}", channel_id),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);

    // 7. 更新凭证（改名；path+body 混合提取约定：body 内亦需携带 id）
    let (status, body) = app
        .put_with_jwt(
            &format!(
                "/api/v1/finance/identity/lark/credentials/{}",
                credential_id
            ),
            &json!({ "id": credential_id, "name": "改名后凭证" }),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data.get("success").and_then(|v| v.as_bool()), Some(true));

    // 8. 引用解除后删除凭证成功
    let (status, body) = app
        .delete_with_jwt(
            &format!(
                "/api/v1/finance/identity/lark/credentials/{}",
                credential_id
            ),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data.get("success").and_then(|v| v.as_bool()), Some(true));

    // 9. 快照归零
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/lark/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert!(
        data["credentials"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "credential removed from snapshot"
    );
}
