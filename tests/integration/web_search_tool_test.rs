//! Integration tests for tavily_search tool & user-level Tavily integration endpoints.
//!
//! Covers：
//! - status 空状态：无凭证返回空列表
//! - 凭证 CRUD 全链路：建 → 快照（key 尾号、明文不回显）→ 设默认 → 改名 → 删默认凭证
//! - 参数校验错误：空名/空 key/未知凭证 → 4xx 引导
//! - 授权解析：经 HTTP 绑定个人 key 后 TavilyDalCredentialResolver 解析出解密明文
//!   （授权单轨走用户凭证库；不发起真实 Tavily 网络调用）
//!
//! 路由：`/api/v1/finance/identity/tavily/`（见 `src/router.rs::finance_routes`）

#[path = "../common/mod.rs"]
mod common;

use ai_orz::pkg::RequestContext;
use ai_orz::pkg::tool_registry::tavily_search::TavilyCredentialResolver;
use serde_json::json;
use sqlx::SqlitePool;

/// 聚合快照端点：无凭证时返回空 credentials（shared_key_configured 已随 D27 删除）
#[sqlx::test]
async fn test_tavily_integration_status_empty(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/tavily/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert!(
        credentials.is_empty(),
        "fresh user has no tavily credentials"
    );
    assert!(
        data.get("shared_key_configured").is_none(),
        "shared_key_configured field should be removed (D27): {}",
        body
    );
}

/// 凭证生命周期：建 → 快照 → 设默认 → 改名 → 删默认凭证（明文 key 全程不回显）
#[sqlx::test]
async fn test_tavily_credential_crud_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let key_plain = "tvly-test-abc123xyz999";

    // 建凭证 #1
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "个人号", "api_key": key_plain }),
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
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "团队号", "api_key": "tvly-test-def456uvw888" }),
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
        .get_with_jwt("/api/v1/finance/identity/tavily/status", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert_eq!(credentials.len(), 2, "should have 2 credentials: {}", body);
    assert!(
        !body.to_string().contains(key_plain),
        "api key plaintext must never be echoed"
    );
    let first = &credentials[0];
    assert_eq!(
        first.get("api_key_tail").and_then(|v| v.as_str()),
        Some("z999"),
        "api_key_tail should be last 4 chars: {}",
        body
    );

    // 设默认 #2 → 快照 is_default 标记正确
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials/default",
            &json!({ "credential_id": cred2 }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/tavily/status", &jwt)
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
            &format!("/api/v1/finance/identity/tavily/credentials/{}", cred1),
            &json!({ "id": cred1, "name": "备用号" }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/tavily/status", &jwt)
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
    assert_eq!(renamed, "备用号");

    // 删默认凭证 #2 → 快照仅剩 #1 且默认标记随删除清除
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/identity/tavily/credentials/{}", cred2),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/identity/tavily/status", &jwt)
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

/// 参数校验：空名/空 key → 4xx；未知凭证删/设默认 → 4xx
#[sqlx::test]
async fn test_tavily_credential_validation_errors(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 空名
    let (status, _) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "  ", "api_key": "tvly-x" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty name should 4xx, got {}",
        status
    );

    // 空 key
    let (status, _) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "n", "api_key": "" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty api key should 4xx, got {}",
        status
    );

    // 删未知凭证
    let (status, _) = app
        .delete_with_jwt(
            "/api/v1/finance/identity/tavily/credentials/no-such-cred",
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
            "/api/v1/finance/identity/tavily/credentials/default",
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

/// 授权解析：经 HTTP 绑定个人 key 后 TavilyDalCredentialResolver
/// 能按用户解析出解密明文（默认凭证优先），且默认槽位轮换后解析结果跟随切换。
/// 不发起真实 Tavily 网络调用。
#[sqlx::test]
async fn test_tavily_credential_resolver_default_rotation(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 建两条凭证
    let key1 = "tvly-resolve-key-1111aaaa";
    let key2 = "tvly-resolve-key-2222bbbb";
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "一号", "api_key": key1 }),
            &jwt,
        )
        .await;
    let cred1 = crate::common::assert_api_ok(status, &body)
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id")
        .to_string();
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials",
            &json!({ "name": "二号", "api_key": key2 }),
            &jwt,
        )
        .await;
    let cred2 = crate::common::assert_api_ok(status, &body)
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id 2")
        .to_string();

    // 从登录用户构造带用户上下文的 RequestContext（resolver 依赖 ctx.user_id）
    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // 未设默认 → 回退第一条（cred1/key1）
    let resolver = ai_orz::service::dal::user::TavilyDalCredentialResolver;
    let resolved = resolver
        .resolve(&ctx)
        .await
        .unwrap()
        .expect("resolved key1");
    assert_eq!(resolved, key1);

    // 设默认 #2 → 解析跟随切换为 key2
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/identity/tavily/credentials/default",
            &json!({ "credential_id": cred2 }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let resolved = resolver
        .resolve(&ctx)
        .await
        .unwrap()
        .expect("resolved key2");
    assert_eq!(resolved, key2);

    // 删默认凭证 #2 → 回退第一条 key1
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/identity/tavily/credentials/{}", cred2),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let resolved = resolver
        .resolve(&ctx)
        .await
        .unwrap()
        .expect("resolved key1 again");
    assert_eq!(resolved, key1);

    // 删光后 → None（单轨授权无兜底，未绑定即缺）
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/finance/identity/tavily/credentials/{}", cred1),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    assert!(resolver.resolve(&ctx).await.unwrap().is_none());
}
