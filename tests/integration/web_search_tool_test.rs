//! Integration tests for generic-token integration endpoints (platform=tavily).
//!
//! Covers：
//! - status 空状态：无凭证返回空列表
//! - 凭证 CRUD 全链路：建 → 快照（token 尾号、明文不回显）→ 设默认 → 改名 → 删默认凭证
//! - 参数校验错误：空名/空 token/未知凭证 → 4xx 引导
//! - 授权解析：经 HTTP 绑定个人 token 后按 D17 编排链（user dal find_default +
//!   pkg resolve_requirements）解析出解密明文（授权单轨走用户凭证库；
//!   不发起真实 Tavily 网络调用）
//!
//! 路由：`/api/v1/finance/identity/generic-token/`（见 `src/router.rs::finance_routes`）
//! 本测试以 platform=tavily 作为 GenericToken 多平台共用链路的代表用例。

#[path = "../common/mod.rs"]
mod common;

use ::common::models::CredentialKind;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::credential::{FetchedCredential, resolve_requirements};
use ai_orz::pkg::tool_registry::BuiltinToolFactory;
use ai_orz::pkg::tool_registry::tavily_search::TavilySearchToolFactory;
use serde_json::json;
use sqlx::SqlitePool;

const PLATFORM: &str = "tavily";
const BASE: &str = "/api/v1/finance/identity/generic-token";

/// 聚合快照端点：无凭证时返回空 credentials
#[sqlx::test]
async fn test_tavily_integration_status_empty(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let (status, body) = app
        .get_with_jwt(&format!("{BASE}/status?platform={PLATFORM}"), &jwt)
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
}

/// 凭证生命周期：建 → 快照 → 设默认 → 改名 → 删默认凭证（明文 token 全程不回显）
#[sqlx::test]
async fn test_tavily_credential_crud_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let token_plain = "tvly-test-abc123xyz999";

    // 建凭证 #1
    let (status, body) = app
        .post_with_jwt(
            &format!("{BASE}/credentials"),
            &json!({ "name": "个人号", "platform": PLATFORM, "api_token": token_plain }),
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
            &format!("{BASE}/credentials"),
            &json!({ "name": "团队号", "platform": PLATFORM, "api_token": "tvly-test-def456uvw888" }),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let cred2 = data
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id 2")
        .to_string();

    // 快照：两条、无显式默认、尾号正确、明文不回显、platform 字段正确
    let (status, body) = app
        .get_with_jwt(&format!("{BASE}/status?platform={PLATFORM}"), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let credentials = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array");
    assert_eq!(credentials.len(), 2, "should have 2 credentials: {}", body);
    assert!(
        !body.to_string().contains(token_plain),
        "api token plaintext must never be echoed"
    );
    let first = &credentials[0];
    assert_eq!(
        first.get("api_token_tail").and_then(|v| v.as_str()),
        Some("z999"),
        "api_token_tail should be last 4 chars: {}",
        body
    );
    assert_eq!(
        first.get("platform").and_then(|v| v.as_str()),
        Some(PLATFORM),
        "snapshot should carry platform: {}",
        body
    );

    // 设默认 #2 → 快照 is_default 标记正确
    let (status, body) = app
        .post_with_jwt(
            &format!("{BASE}/credentials/default"),
            &json!({ "platform": PLATFORM, "credential_id": cred2 }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt(&format!("{BASE}/status?platform={PLATFORM}"), &jwt)
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

    // 改名 #1（PATCH，platform 已由 credential id 唯一确定，body 不带 platform）
    let (status, body) = app
        .patch_with_jwt(
            &format!("{BASE}/credentials/{}", cred1),
            &json!({ "id": cred1, "name": "备用号" }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt(&format!("{BASE}/status?platform={PLATFORM}"), &jwt)
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
            &format!("{BASE}/credentials/{}", cred2),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let (status, body) = app
        .get_with_jwt(&format!("{BASE}/status?platform={PLATFORM}"), &jwt)
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

/// 参数校验：空名/空 token/空 platform → 4xx；未知凭证删/设默认 → 4xx
#[sqlx::test]
async fn test_tavily_credential_validation_errors(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 空名
    let (status, _) = app
        .post_with_jwt(
            &format!("{BASE}/credentials"),
            &json!({ "name": "  ", "platform": PLATFORM, "api_token": "tvly-x" }),
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
            &format!("{BASE}/credentials"),
            &json!({ "name": "n", "platform": PLATFORM, "api_token": "" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty api token should 4xx, got {}",
        status
    );

    // 空 platform
    let (status, _) = app
        .post_with_jwt(
            &format!("{BASE}/credentials"),
            &json!({ "name": "n", "platform": "  ", "api_token": "tvly-x" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "empty platform should 4xx, got {}",
        status
    );

    // 删未知凭证
    let (status, _) = app
        .delete_with_jwt(
            &format!("{BASE}/credentials/no-such-cred"),
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
            &format!("{BASE}/credentials/default"),
            &json!({ "platform": PLATFORM, "credential_id": "no-such-cred" }),
            &jwt,
        )
        .await;
    assert!(
        status.is_client_error(),
        "unknown credential default should 4xx, got {}",
        status
    );
}

/// 按 D17 编排链解析用户默认 Tavily token（domain `resolve_tool_credentials`
/// 的 GenericToken+platform=tavily 路由等价组合：user dal find_default → pkg resolve_requirements
/// 解密 + canonical 取值）；未绑定返回 None。
async fn resolve_tavily_key(ctx: &RequestContext, user_id: &str) -> Option<String> {
    let credential = ai_orz::service::dal::user::dal()
        .find_default_credential(
            ctx.clone(),
            user_id,
            CredentialKind::GenericToken,
            Some(PLATFORM),
        )
        .await
        .unwrap()?;
    let fetched = FetchedCredential {
        credential_id: credential.id().to_string(),
        detail: credential.detail().clone(),
        attributes: Default::default(),
        already_decrypted: false,
    };
    let requirements = TavilySearchToolFactory.credential_requirements();
    let resolved = resolve_requirements(&requirements, &[fetched])
        .await
        .unwrap();
    resolved.first().map(|r| r.value.clone())
}

/// 授权解析：经 HTTP 绑定个人 token 后按 D17 编排链
/// 能按用户解析出解密明文（默认凭证优先），且默认槽位轮换后解析结果跟随切换。
/// 不发起真实 Tavily 网络调用。
#[sqlx::test]
async fn test_tavily_credential_resolution_default_rotation(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 建两条凭证
    let key1 = "tvly-resolve-key-1111aaaa";
    let key2 = "tvly-resolve-key-2222bbbb";
    let (status, body) = app
        .post_with_jwt(
            &format!("{BASE}/credentials"),
            &json!({ "name": "一号", "platform": PLATFORM, "api_token": key1 }),
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
            &format!("{BASE}/credentials"),
            &json!({ "name": "二号", "platform": PLATFORM, "api_token": key2 }),
            &jwt,
        )
        .await;
    let cred2 = crate::common::assert_api_ok(status, &body)
        .get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id 2")
        .to_string();

    // 从登录用户构造带用户上下文的 RequestContext（解析依赖 ctx.user_id）
    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // 未设默认 → 回退第一条（cred1/key1）
    let resolved = resolve_tavily_key(&ctx, &bs.user_id)
        .await
        .expect("resolved key1");
    assert_eq!(resolved, key1);

    // 设默认 #2 → 解析跟随切换为 key2
    let (status, body) = app
        .post_with_jwt(
            &format!("{BASE}/credentials/default"),
            &json!({ "platform": PLATFORM, "credential_id": cred2 }),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let resolved = resolve_tavily_key(&ctx, &bs.user_id)
        .await
        .expect("resolved key2");
    assert_eq!(resolved, key2);

    // 删默认凭证 #2 → 回退第一条 key1
    let (status, body) = app
        .delete_with_jwt(
            &format!("{BASE}/credentials/{}", cred2),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    let resolved = resolve_tavily_key(&ctx, &bs.user_id)
        .await
        .expect("resolved key1 again");
    assert_eq!(resolved, key1);

    // 删光后 → None（单轨授权无兜底，未绑定即缺）
    let (status, body) = app
        .delete_with_jwt(
            &format!("{BASE}/credentials/{}", cred1),
            &jwt,
        )
        .await;
    crate::common::assert_api_ok(status, &body);
    assert!(
        resolve_tavily_key(&ctx, &bs.user_id).await.is_none(),
        "no credential left → resolution miss"
    );
}
