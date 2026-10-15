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
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭据持有者改为全新注册成员：其名下凭据从零开始，不受共享库中
    // sibling 用例向复用管理员写入凭据/切默认状态的影响。
    let (jwt, user_id, _member_org) = crate::common::factories::register_fresh_member(&app).await;

    let tvly_key = "tvly-multi-platform-test-key";
    let doubao_key = "doubao-multi-platform-test-key";

    let tvly_cred_id = create_generic_token(&app, &jwt, "Tavily key", "tavily", tvly_key).await;
    let doubao_cred_id =
        create_generic_token(&app, &jwt, "Doubao key", "doubao_search", doubao_key).await;

    set_default(&app, &jwt, "tavily", &tvly_cred_id).await;
    set_default(&app, &jwt, "doubao_search", &doubao_cred_id).await;

    let ctx = RequestContext::builder().user_id(user_id.clone()).build();

    // T1: tavily 需求 → 解析出 tavily key
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
        .await
        .expect("tavily resolution should succeed");
    assert_eq!(resolved, tvly_key);

    // T2: doubao_search 需求 → 解析出 doubao key
    let resolved = resolve_platform_token(&ctx, &user_id, "doubao_search", &doubao_requirements())
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
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭据持有者改为全新注册成员：隔离 sibling 用例对复用管理员名下
    // tavily/doubao_search 凭据的相互覆盖。
    let (jwt, user_id, _member_org) = crate::common::factories::register_fresh_member(&app).await;

    let tvly_key = "tvly-isolation-test-key";

    // 只创建 tavily 凭据
    let _tvly_cred_id =
        create_generic_token(&app, &jwt, "Tavily key only", "tavily", tvly_key).await;

    let ctx = RequestContext::builder().user_id(user_id.clone()).build();

    // T1: tavily 需求 → 能解析
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
        .await
        .expect("tavily should resolve");
    assert_eq!(resolved, tvly_key);

    // T2: doubao_search 需求 → 不能解析（跨平台隔离）
    let resolved =
        resolve_platform_token(&ctx, &user_id, "doubao_search", &doubao_requirements()).await;
    assert!(
        resolved.is_none(),
        "doubao_search requirement should NOT match tavily-only credential"
    );

    // 反向：创建 doubao 凭据
    let doubao_key = "doubao-isolation-test-key";
    let _doubao_cred_id =
        create_generic_token(&app, &jwt, "Doubao key only", "doubao_search", doubao_key).await;

    // T3: doubao 需求 → 能解析
    let resolved = resolve_platform_token(&ctx, &user_id, "doubao_search", &doubao_requirements())
        .await
        .expect("doubao should resolve");
    assert_eq!(resolved, doubao_key);

    // T4: tavily 需求 → 仍然解析到自己的（不会被 doubao 污染）
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
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
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 解析主体改为全新注册成员：复用管理员的共享身份可能已被 sibling 用例
    // 写入凭据，「无凭据」断言必须落在真正零数据的身份上才确定成立。
    let (_member_jwt, member_id, _member_org) =
        crate::common::factories::register_fresh_member(&app).await;

    let ctx = RequestContext::builder().user_id(member_id.clone()).build();

    // 新成员无凭据 → 两种平台都解析不到
    assert!(
        resolve_platform_token(&ctx, &member_id, "tavily", &tavily_requirements())
            .await
            .is_none()
    );
    assert!(
        resolve_platform_token(&ctx, &member_id, "doubao_search", &doubao_requirements(),)
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
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭据持有者改为全新注册成员：默认切换语义（含「未设默认回退第一条」）
    // 必须基于本用例自己写入的凭据集合，不受 sibling 用例残留状态影响。
    let (jwt, user_id, _member_org) = crate::common::factories::register_fresh_member(&app).await;

    let key1 = "tvly-switch-key-1111";
    let key2 = "tvly-switch-key-2222";

    let _cred1_id = create_generic_token(&app, &jwt, "一号", "tavily", key1).await;
    let cred2_id = create_generic_token(&app, &jwt, "二号", "tavily", key2).await;

    let ctx = RequestContext::builder().user_id(user_id.clone()).build();

    // 未设默认 → 回退第一条
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
        .await
        .expect("should resolve key1");
    assert_eq!(resolved, key1);

    // 设默认 → 解析跟随切换
    set_default(&app, &jwt, "tavily", &cred2_id).await;
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
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
    let resolved = resolve_platform_token(&ctx, &user_id, "tavily", &tavily_requirements())
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
    // 满足 register_fresh_member 前置条件（Local 组织已初始化）
    let _ = crate::common::factories::bootstrap_and_login(&app).await;

    // 凭据持有者改为全新注册成员：确保 find_default_credential 精确命中
    // 本用例刚写入的凭据，避免 sibling 用例向复用管理员写入的凭据干扰。
    let (jwt, user_id, _member_org) = crate::common::factories::register_fresh_member(&app).await;

    let api_token = "tvly-field-integrity-key";
    let _cred_id = create_generic_token(&app, &jwt, "Field integrity", "tavily", api_token).await;

    let ctx = RequestContext::builder().user_id(user_id.clone()).build();

    let credential = ai_orz::service::dal::user::dal()
        .find_default_credential(
            ctx.clone(),
            &user_id,
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

// ==================== status 端点 platform 可选化（空串=全量，TL 任务书 B-2③） ====================

/// status 端点便捷调用：query 形态 "?platform="（空串全量）或 "?platform=xxx"（精确过滤），返回 credentials 数组
async fn status_credentials(
    app: &common::TestApp,
    jwt: &str,
    query: &str,
) -> Vec<serde_json::Value> {
    let path = format!("{GENERIC_TOKEN_BASE}/status{query}");
    let (status, body) = app.get_with_jwt(&path, jwt).await;
    let data = common::assert_api_ok(status, &body);
    data.get("credentials")
        .and_then(|v| v.as_array())
        .expect("credentials array")
        .clone()
}

/// 不传/空串 platform → 全量：多 platform（含自定义 ledger）凭证全部返回，
/// 快照 platform 字段回填凭证自身实值，token 永不回显仅尾号
#[sqlx::test]
async fn test_generic_token_status_empty_platform_returns_all(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let _ = crate::common::factories::bootstrap_and_login(&app).await;
    let (jwt, _user_id, _org) = crate::common::factories::register_fresh_member(&app).await;

    create_generic_token(&app, &jwt, "Tavily key", "tavily", "tvly-status-all-key").await;
    create_generic_token(
        &app,
        &jwt,
        "Doubao key",
        "doubao_search",
        "doubao-status-all-key",
    )
    .await;
    create_generic_token(&app, &jwt, "Ledger key", "ledger", "ledger-status-all-key").await;

    // 显式空串 platform（?platform=）→ 全量（沿 email/status「空串=全量」同构口径；
    // 实际调用链=单 A 前端封装 get_status("") 恒显式拼参，见完工回报披露）
    let all = status_credentials(&app, &jwt, "?platform=").await;
    assert_eq!(all.len(), 3, "空串 platform 应回全部 3 条凭证");
    let platforms: Vec<&str> = all
        .iter()
        .filter_map(|c| c.get("platform").and_then(|v| v.as_str()))
        .collect();
    assert!(platforms.contains(&"tavily") && platforms.contains(&"doubao_search"));
    assert!(
        platforms.contains(&"ledger"),
        "自定义 platform=ledger 应可枚举（单 A 自定义凭证区块数据源）"
    );

    // token 永不回显：仅尾号 4 位
    for c in &all {
        let tail = c
            .get("api_token_tail")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(tail.chars().count(), 4, "api_token_tail 仅尾号 4 位");
        assert!(!tail.contains("status-all-key"), "token 明文不得回显");
    }
}

/// 传 platform → 原精确过滤语义零变更；未匹配 platform 返回空数组非报错
#[sqlx::test]
async fn test_generic_token_status_filter_by_platform(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let _ = crate::common::factories::bootstrap_and_login(&app).await;
    let (jwt, _user_id, _org) = crate::common::factories::register_fresh_member(&app).await;

    create_generic_token(&app, &jwt, "Tavily key", "tavily", "tvly-status-filter-key").await;
    create_generic_token(
        &app,
        &jwt,
        "Ledger key",
        "ledger",
        "ledger-status-filter-key",
    )
    .await;

    let tavily = status_credentials(&app, &jwt, "?platform=tavily").await;
    assert_eq!(tavily.len(), 1, "过滤语义零变更：仅返回匹配 platform");
    assert_eq!(
        tavily[0].get("platform").and_then(|v| v.as_str()),
        Some("tavily")
    );
    assert_eq!(
        tavily[0].get("name").and_then(|v| v.as_str()),
        Some("Tavily key")
    );

    let none = status_credentials(&app, &jwt, "?platform=nonexistent").await;
    assert!(none.is_empty(), "未匹配 platform 返回空数组非报错");
}

/// 零凭证用户：全量与过滤均返回空数组（200 非报错）
#[sqlx::test]
async fn test_generic_token_status_empty_credentials_ok(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let _ = crate::common::factories::bootstrap_and_login(&app).await;
    let (jwt, _user_id, _org) = crate::common::factories::register_fresh_member(&app).await;

    let all = status_credentials(&app, &jwt, "?platform=").await;
    assert!(all.is_empty(), "零凭证用户全量查询返回空数组非报错");
    let filtered = status_credentials(&app, &jwt, "?platform=tavily").await;
    assert!(filtered.is_empty());
}

/// 跨用户隔离：owned_credential_query 只见本人凭证
#[sqlx::test]
async fn test_generic_token_status_user_isolation(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let _ = crate::common::factories::bootstrap_and_login(&app).await;
    let (jwt_a, _ua, _oa) = crate::common::factories::register_fresh_member(&app).await;
    let (jwt_b, _ub, _ob) = crate::common::factories::register_fresh_member(&app).await;

    create_generic_token(&app, &jwt_a, "A tavily", "tavily", "tvly-isol-key").await;

    let b_view = status_credentials(&app, &jwt_b, "?platform=").await;
    assert!(b_view.is_empty(), "跨用户隔离：B 不得见到 A 的凭证");
    let a_view = status_credentials(&app, &jwt_a, "?platform=tavily").await;
    assert_eq!(a_view.len(), 1, "本人查询不受隔离影响");
}
