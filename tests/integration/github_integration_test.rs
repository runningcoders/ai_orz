//! Integration tests for the user-level GitHub integration endpoints.
//!
//! Covers（gh 二进制存在与否两种环境均可跑）：
//! - status 空状态：无凭证返回空列表 + auth 降级 logged_in=false（HOME 无登录态，不 5xx）
//! - 凭证 CRUD 全链路：建 → 快照（token 尾号、明文不回显）→ 设默认 → 改名 → 删生效凭证
//! - 参数校验错误：空名/空 token/未知凭证 → 4xx 引导
//!
//! 路由：`/api/v1/finance/identity/github/`（见 `src/router.rs::finance_routes`）

#[path = "../common/mod.rs"]
mod common;

use serde_json::json;
use sqlx::SqlitePool;

/// 聚合快照端点：无凭证时返回空 credentials + auth 降级（不 5xx）
#[sqlx::test]
async fn test_github_integration_status_empty(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, _admin_jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 共享 DB 下复用的 SuperAdmin 可能残留其他用例的凭证，
    // 用邀请码注册一个真正全新的成员作为空状态主体
    let (member_jwt, _member_id, _member_org) =
        crate::common::factories::register_fresh_member(&app).await;

    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/github/status", &member_jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert!(
        credentials.is_empty(),
        "fresh user has no github credentials"
    );
    // 测试 HOME（临时目录）下无 gh 登录态 → 必然未登录
    assert_eq!(
        data.get("auth")
            .and_then(|v| v.get("logged_in"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "auth should degrade to logged_in=false instead of 5xx: {}",
        body
    );
}

/// 凭证生命周期：建 → 快照 → 设默认 → 改名 → 删生效凭证（明文 token 全程不回显）
#[sqlx::test]
async fn test_github_credential_crud_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭证持有者改为全新注册成员：快照条数 / 默认标记等绝对断言只统计
    // 本用例写入的凭据，不受 sibling 用例向复用管理员累积数据的影响。
    let (jwt, _member_id, _member_org) =
        crate::common::factories::register_fresh_member(&app).await;

    let token_plain = "ghp_test_abc123xyz999";

    // 建凭证 #1
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials",
            &json!({ "name": "工作号", "token": token_plain }),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let cred1 = data
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id")
        .to_string();

    // 建凭证 #2
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials",
            &json!({ "name": "个人号", "token": "ghp_test_def456uvw888" }),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let cred2 = data
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id 2")
        .to_string();

    // 快照：两条、无显式默认、尾号正确、明文不回显
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/github/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert_eq!(credentials.len(), 2, "should have 2 credentials: {}", body);
    assert!(
        !body.to_string().contains(token_plain),
        "token plaintext must never be echoed"
    );
    let first = &credentials[0];
    assert_eq!(
        first.get("token_tail").and_then(|v| v.as_str()),
        Some("z999"),
        "token_tail should be last 4 chars: {}",
        body
    );

    // 设默认 #2 → 快照 is_default 标记正确
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials/default",
            &json!({ "credential_id": cred2 }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/github/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    let cred2_snapshot = credentials
        .iter()
        .find(|c| c.get("credential_id").and_then(|v| v.as_str()) == Some(cred2.as_str()))
        .expect("cred2 snapshot");
    assert_eq!(
        cred2_snapshot.get("is_default").and_then(|v| v.as_bool()),
        Some(true),
        "cred2 should be default: {}",
        body
    );

    // 改名 #1（path+body 混合提取约定：body 内亦需携带 id）
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/finance/identity/github/credentials/{}", cred1),
            &json!({ "id": cred1, "name": "开源号" }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/github/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let renamed = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array")
        .iter()
        .find(|c| c.get("credential_id").and_then(|v| v.as_str()) == Some(cred1.as_str()))
        .expect("cred1 snapshot")
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    assert_eq!(renamed, "开源号");

    // 删默认（生效）凭证 #2 → 快照仅剩 #1 且默认标记随删除清除
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/identity/github/credentials/{}", cred2),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/github/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert_eq!(credentials.len(), 1, "only cred1 remains: {}", body);
    assert_eq!(
        credentials[0].get("is_default").and_then(|v| v.as_bool()),
        Some(false),
        "default flag should be cleared with deletion: {}",
        body
    );
}

/// 参数校验：空名/空 token → 4xx；未知凭证删/设默认 → 4xx
#[sqlx::test]
async fn test_github_credential_validation_errors(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 空名
    let (status, _) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials",
            &json!({ "name": "  ", "token": "ghp_x" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty name should 4xx, got {}",
        status
    );

    // 空 token
    let (status, _) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials",
            &json!({ "name": "n", "token": "" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty token should 4xx, got {}",
        status
    );

    // 删未知凭证
    let (status, _) = app
        .delete_with_jwt(
            "/api/v1/finance/identity/github/credentials/no-such-cred",
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "unknown credential delete should 4xx, got {}",
        status
    );

    // 默认指向未知凭证
    let (status, _) = app
        .post_with_jwt(
            "/api/v1/finance/identity/github/credentials/default",
            &json!({ "credential_id": "no-such-cred" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "unknown credential default should 4xx, got {}",
        status
    );
}
