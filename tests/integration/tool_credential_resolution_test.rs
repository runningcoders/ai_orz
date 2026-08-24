//! Integration tests for GenericToken credential resolution at the tool-call boundary.
//!
//! Covers：
//! - GenericToken 多平台 CRUD（tavily / doubao_search）
//! - 跨平台隔离：credential platform 与 tool requirement platform 必须匹配
//! - 完整 DAL→pkg 解析链路：find_default_credential → resolve_requirements
//! - 凭据缺失 → 解析返回 None（调用方据此出 credential_missing 引导）
//! - 多凭据 + 默认切换后的解析跟随
//! - 解析结果 ResolvedRequirement 字段完整性

#[path = "../common/mod.rs"]
mod common;

use ::common::models::{CredentialBinding, CredentialKind};
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::credential::{FetchedCredential, resolve_requirements};
use ai_orz::pkg::tool_registry::BuiltinToolFactory;
use ai_orz::pkg::tool_registry::doubao_search::DoubaoSearchToolFactory;
use ai_orz::pkg::tool_registry::tavily_search::TavilySearchToolFactory;
use serde_json::json;
use sqlx::SqlitePool;

const GENERIC_TOKEN_BASE: &str = "/api/v1/finance/identity/generic-token";

// ==================== 辅助函数 ====================

async fn create_generic_token(
    app: &common::TestApp,
    jwt: &str,
    name: &str,
    platform: &str,
    api_token: &str,
) -> String {
    let (status, body) = app
        .post_with_jwt(
            &format!("{GENERIC_TOKEN_BASE}/credentials"),
            &json!({ "name": name, "platform": platform, "api_token": api_token }),
            jwt,
        )
        .await;
    let data = common::assert_api_ok(status, &body);
    data.get("credential_id")
        .and_then(|v| v.as_str())
        .expect("credential_id")
        .to_string()
}

async fn set_default(app: &common::TestApp, jwt: &str, platform: &str, credential_id: &str) {
    let (status, body) = app
        .post_with_jwt(
            &format!("{GENERIC_TOKEN_BASE}/credentials/default"),
            &json!({ "platform": platform, "credential_id": credential_id }),
            jwt,
        )
        .await;
    common::assert_api_ok(status, &body);
}

async fn resolve_platform_token(
    ctx: &RequestContext,
    user_id: &str,
    platform: &str,
    requirements: &[::common::models::CredentialRequirement],
) -> Option<String> {
    let credential = ai_orz::service::dal::user::dal()
        .find_default_credential(
            ctx.clone(),
            user_id,
            CredentialKind::GenericToken,
            Some(platform),
        )
        .await
        .unwrap()?;
    let fetched = FetchedCredential {
        credential_id: credential.id().to_string(),
        detail: credential.detail().clone(),
        attributes: Default::default(),
        already_decrypted: false,
    };
    let resolved = resolve_requirements(requirements, &[fetched])
        .await
        .unwrap();
    resolved.first().map(|r| r.value.clone())
}

fn tavily_requirements() -> Vec<::common::models::CredentialRequirement> {
    TavilySearchToolFactory.credential_requirements()
}

fn doubao_requirements() -> Vec<::common::models::CredentialRequirement> {
    DoubaoSearchToolFactory.credential_requirements()
}

// ==================== 多平台 CRUD + 解析 ====================

/// GenericToken 支持多平台共存，各自独立 CRUD + 解析
#[sqlx::test]
async fn test_generic_token_multi_platform_crud_and_resolution(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let tvly_key = "tvly-multi-platform-test-key";
    let doubao_key = "doubao-multi-platform-test-key";

    let tvly_cred_id = create_generic_token(&app, &jwt, "Tavily key", "tavily", tvly_key).await;
    let doubao_cred_id =
        create_generic_token(&app, &jwt, "Doubao key", "doubao_search", doubao_key).await;

    set_default(&app, &jwt, "tavily", &tvly_cred_id).await;
    set_default(&app, &jwt, "doubao_search", &doubao_cred_id).await;

    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // T1: tavily 需求 → 解析出 tavily key
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("tavily resolution should succeed");
    assert_eq!(resolved, tvly_key);

    // T2: doubao_search 需求 → 解析出 doubao key
    let resolved =
        resolve_platform_token(&ctx, &bs.user_id, "doubao_search", &doubao_requirements())
            .await
            .expect("doubao resolution should succeed");
    assert_eq!(resolved, doubao_key);

    // T3: 状态快照验证各自独立
    let (status, body) = app
        .get_with_jwt(
            &format!("{GENERIC_TOKEN_BASE}/status?platform=tavily"),
            &jwt,
        )
        .await;
    let data = common::assert_api_ok(status, &body);
    let tavily_creds = data
        .get("credentials")
        .and_then(|v| v.as_array())
        .expect("tavily credentials");
    assert_eq!(tavily_creds.len(), 1);
    assert_eq!(
        tavily_creds[0].get("platform").and_then(|v| v.as_str()),
        Some("tavily")
    );
    assert_eq!(
        tavily_creds[0].get("is_default").and_then(|v| v.as_bool()),
        Some(true)
    );
}

// ==================== 跨平台隔离 ====================

/// platform 隔离：A 平台的凭据不会被 B 平台的需求匹配
#[sqlx::test]
async fn test_cross_platform_isolation(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let tvly_key = "tvly-isolation-test-key";

    // 只创建 tavily 凭据
    let _tvly_cred_id =
        create_generic_token(&app, &jwt, "Tavily key only", "tavily", tvly_key).await;

    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // T1: tavily 需求 → 能解析
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("tavily should resolve");
    assert_eq!(resolved, tvly_key);

    // T2: doubao_search 需求 → 不能解析（跨平台隔离）
    let resolved =
        resolve_platform_token(&ctx, &bs.user_id, "doubao_search", &doubao_requirements()).await;
    assert!(
        resolved.is_none(),
        "doubao_search requirement should NOT match tavily-only credential"
    );

    // 反向：创建 doubao 凭据
    let doubao_key = "doubao-isolation-test-key";
    let _doubao_cred_id =
        create_generic_token(&app, &jwt, "Doubao key only", "doubao_search", doubao_key).await;

    // T3: doubao 需求 → 能解析
    let resolved =
        resolve_platform_token(&ctx, &bs.user_id, "doubao_search", &doubao_requirements())
            .await
            .expect("doubao should resolve");
    assert_eq!(resolved, doubao_key);

    // T4: tavily 需求 → 仍然解析到自己的（不会被 doubao 污染）
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("tavily should still resolve");
    assert_eq!(resolved, tvly_key);
}

// ==================== 凭据缺失 → 解析返回 None ====================

/// 无任何凭据时，解析返回 None（调用方据此出 credential_missing 引导）
#[sqlx::test]
async fn test_credential_missing_returns_none(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, _jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // 新用户无凭据 → 两种平台都解析不到
    assert!(
        resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
            .await
            .is_none()
    );
    assert!(
        resolve_platform_token(&ctx, &bs.user_id, "doubao_search", &doubao_requirements(),)
            .await
            .is_none()
    );
}

// ==================== 多凭据 + 默认切换 ====================

/// 同一平台多凭据 + 默认切换后的解析跟随
#[sqlx::test]
async fn test_default_credential_switch_follows_resolution(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let key1 = "tvly-switch-key-1111";
    let key2 = "tvly-switch-key-2222";

    let _cred1_id = create_generic_token(&app, &jwt, "一号", "tavily", key1).await;
    let cred2_id = create_generic_token(&app, &jwt, "二号", "tavily", key2).await;

    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    // 未设默认 → 回退第一条
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("should resolve key1");
    assert_eq!(resolved, key1);

    // 设默认 → 解析跟随切换
    set_default(&app, &jwt, "tavily", &cred2_id).await;
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("should resolve key2");
    assert_eq!(resolved, key2);

    // 删除默认 → 回退第一条
    let (status, body) = app
        .delete_with_jwt(
            &format!("{GENERIC_TOKEN_BASE}/credentials/{}", cred2_id),
            &jwt,
        )
        .await;
    common::assert_api_ok(status, &body);
    let resolved = resolve_platform_token(&ctx, &bs.user_id, "tavily", &tavily_requirements())
        .await
        .expect("should resolve key1 after deletion");
    assert_eq!(resolved, key1);
}

// ==================== 解析结果字段完整性 ====================

/// 解析出的 ResolvedRequirement 各字段正确性
#[sqlx::test]
async fn test_resolved_requirement_field_integrity(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let api_token = "tvly-field-integrity-key";
    let _cred_id = create_generic_token(&app, &jwt, "Field integrity", "tavily", api_token).await;

    let ctx = RequestContext::builder()
        .user_id(bs.user_id.clone())
        .build();

    let credential = ai_orz::service::dal::user::dal()
        .find_default_credential(
            ctx.clone(),
            &bs.user_id,
            CredentialKind::GenericToken,
            Some("tavily"),
        )
        .await
        .unwrap()
        .expect("credential should exist");

    let fetched = FetchedCredential {
        credential_id: credential.id().to_string(),
        detail: credential.detail().clone(),
        attributes: Default::default(),
        already_decrypted: false,
    };

    let requirements = tavily_requirements();
    let resolved = resolve_requirements(&requirements, &[fetched])
        .await
        .expect("resolution should succeed");

    assert_eq!(resolved.len(), 1);
    let r = &resolved[0];

    // 值正确
    assert_eq!(r.value, api_token);
    // binding field 正确
    match &r.requirement.binding {
        CredentialBinding::Internal { field } => {
            assert_eq!(field, "api_key");
        }
        _ => panic!("expected Internal binding"),
    }
    // kind 正确
    assert_eq!(r.requirement.kind, CredentialKind::GenericToken);
    // platform 正确
    assert_eq!(r.requirement.platform.as_deref(), Some("tavily"));
}
