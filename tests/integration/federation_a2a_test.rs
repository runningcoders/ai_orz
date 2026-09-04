//! 联邦调用鉴权集成测试（跨组织业务调用方案 P1+P2 验收）。
//!
//! `/a2a` 双模鉴权：
//! - 本地 JWT（既有语义，a2a_flow_test 已覆盖，不在此重复）；
//! - 建联对端节点：`Authorization: Bearer <link access_token>`（哈希匹配
//!   Active 连接 `peer_token_hash`）+ 可选 `X-Federation-Caller` 身份声明。
//!
//! 覆盖：有效凭证 + 声明（caller_user 注入 ctx.user_id）、无声明（合成
//! `federation:{peer_org_id}` 身份）、错凭证 401、非法声明 401（fail-closed）、
//! 声明组织与连接归属不一致 401（防跨连接冒充）、无任何凭证 401。
//!
//! 双节点说明同 organization_link_test：共享全局 Storage 单例，
//! "对端组织" 用独立 org id 表达（逻辑隔离）。

#[path = "../common/mod.rs"]
mod common;

use ::common::api::InitializeSystemRequest;
use ::common::constants::agent_roles::ROLE_A2A_GATEWAY;
use ::common::enums::AgentStatus;
use ai_orz::models::agent::{Agent, AgentPo};
use ai_orz::models::organization_link::OrganizationLinkPo;
use ai_orz::pkg::RequestContext;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Domain 层直接创建测试节点组织 + 登录拿 JWT（规避 /initialize 竞态，
/// 见 organization_link_test::create_node 注释）。
async fn create_node(app: &crate::common::TestApp, tag: &str) -> (String, String) {
    let ctx = RequestContext::from_storage(
        format!("fed-a2a-{tag}").as_str(),
        ai_orz::pkg::storage::get().clone(),
    );
    let username = format!("{tag}-admin-{}", Uuid::now_v7());
    let password = format!("{tag}-pw-{}", Uuid::now_v7());
    let (org_id, _user_id) = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .create_org_and_owner(
            ctx,
            InitializeSystemRequest {
                organization_name: format!("{tag}-Org-{}", Uuid::now_v7()),
                admin_username: username.clone(),
                admin_password: password.clone(),
                description: None,
                admin_display_name: None,
                admin_email: None,
                chat_model: None,
                embedding_model: None,
            },
        )
        .await
        .expect("create node org via domain should succeed");
    let jwt = crate::common::factories::login_and_get_jwt(app, &org_id, &username, &password).await;
    (org_id, jwt)
}

/// 为 org_b 播种一个 Onboarded 的 a2a_gateway Agent（tasks/send 的解析目标）。
async fn seed_gateway_agent(org_b: &str) {
    let ctx = RequestContext::from_storage("fed-a2a-seed", ai_orz::pkg::storage::get().clone())
        .to_builder()
        .organization_id(org_b)
        .build();
    let mut po = AgentPo::new(
        format!("A2A网关-{}", Uuid::now_v7()),
        vec![ROLE_A2A_GATEWAY.to_string()],
        "联邦测试网关".to_string(),
        vec!["chat".to_string()],
        "测试灵魂".to_string(),
        "provider-federation-test".to_string(),
        "fed-a2a-seed".to_string(),
    );
    po.status = AgentStatus::Onboarded;
    ai_orz::service::dal::agent::dal()
        .create(ctx.clone(), &Agent::from_po(po))
        .await
        .expect("seed gateway agent failed");
}

/// 播种一条 Active 连接（local=org_b 收，peer=org_a 发）：
/// peer_token_hash = sha256(credential)，即 A 出站调 B 时携带 credential。
async fn seed_link(org_b: &str, org_a: &str, credential: &str, capabilities: &str) {
    let ctx = RequestContext::from_storage("fed-a2a-seed", ai_orz::pkg::storage::get().clone());
    let mut link = OrganizationLinkPo::new(
        Uuid::now_v7().to_string(),
        org_b.to_string(),
        org_a.to_string(),
        "https://peer-a.example.com".to_string(),
        "b-side-access-token".to_string(), // B 存的 A 的出站凭证（本测试不使用）
        sha256::digest(credential.as_bytes()),
        "fed-a2a-seed".to_string(),
    );
    // 覆盖默认白名单（Po::new 默认 ["a2a_task"]）
    link.capabilities = capabilities.to_string();
    ai_orz::service::dao::organization_link::dao()
        .insert(ctx, &link)
        .await
        .expect("seed link failed");
}

/// 组装联邦调用请求 headers（Bearer 契约凭证 + 可选声明）
fn federation_headers(credential: &str, declaration: Option<&String>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", credential)).expect("valid bearer"),
    );
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if let Some(decl) = declaration {
        headers.insert(
            axum::http::header::HeaderName::from_static("x-federation-caller"),
            HeaderValue::from_str(decl).expect("valid declaration"),
        );
    }
    headers
}

fn send_task_rpc() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "tasks/send",
        "params": {
            "id": format!("client-task-{}", Uuid::now_v7()),
            "message": {
                "role": "user",
                "parts": [{"type": "text", "text": "跨组织你好"}]
            }
        }
    })
}

/// 有效凭证 + 完整声明：鉴权通过，ctx.user_id = 声明的 caller_user，
/// project 落在目标组织 B（organization_id = B，caller org = A 走日志维度）。
#[sqlx::test]
async fn test_federation_call_with_declaration_creates_task(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "feda").await;
    let (org_b, _jwt_b) = create_node(&app, "fedb").await;
    seed_gateway_agent(&org_b).await;

    let credential = "a".repeat(64);
    seed_link(&org_b, &org_a, &credential, r#"["a2a_task"]"#).await;

    let declaration = serde_json::json!({
        "caller_org": org_a,
        "caller_user": format!("user-a-{}", Uuid::now_v7()),
        "caller_agent": format!("agent-a-{}", Uuid::now_v7()),
    })
    .to_string();

    let (status, body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&credential, Some(&declaration)),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "federation call should pass auth, body: {}",
        body
    );
    let task_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks/send should return result.id")
        .to_string();

    // ctx 注入断言：created_by = 声明的 caller_user（organization/caller org 维度走日志）
    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let project = ai_orz::service::dao::project::dao()
        .find_by_id(ctx, &task_id)
        .await
        .expect("query project failed")
        .expect("project should exist for a2a task");
    let expected_user: serde_json::Value = serde_json::from_str(&declaration).unwrap();
    assert_eq!(
        project.created_by,
        expected_user["caller_user"].as_str().unwrap(),
        "ctx.user_id should come from declaration caller_user"
    );
}

/// 有效凭证 + 无声明：连接级匿名调用，合成身份 federation:{peer_org_id}。
#[sqlx::test]
async fn test_federation_call_without_declaration_uses_synthetic_identity(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "anona").await;
    let (org_b, _jwt_b) = create_node(&app, "anonb").await;
    seed_gateway_agent(&org_b).await;

    let credential = "b".repeat(64);
    seed_link(&org_b, &org_a, &credential, r#"["a2a_task"]"#).await;

    let (status, body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&credential, None),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body: {}", body);
    let task_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .expect("result.id")
        .to_string();

    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let project = ai_orz::service::dao::project::dao()
        .find_by_id(ctx, &task_id)
        .await
        .expect("query project failed")
        .expect("project should exist");
    assert_eq!(
        project.created_by,
        format!("federation:{}", org_a),
        "anonymous connection-level call uses synthetic identity"
    );
}

/// 错误凭证：哈希不匹配任何 Active 连接 → 401（防枚举，与无效凭证同响应）。
#[sqlx::test]
async fn test_federation_call_with_wrong_credential_is_401(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "wronga").await;
    let (org_b, _jwt_b) = create_node(&app, "wrongb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, &"c".repeat(64), r#"["a2a_task"]"#).await;

    let (status, body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&"d".repeat(64), None),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body: {}", body);
}

/// 声明头存在但非法 JSON → 401（fail-closed，防注入半可信声明）。
#[sqlx::test]
async fn test_federation_call_with_malformed_declaration_is_401(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "malfa").await;
    let (org_b, _jwt_b) = create_node(&app, "malfb").await;
    seed_gateway_agent(&org_b).await;
    let credential = "e".repeat(64);
    seed_link(&org_b, &org_a, &credential, r#"["a2a_task"]"#).await;

    let bad_declaration = "not-json".to_string();
    let (status, _body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&credential, Some(&bad_declaration)),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// 声明 caller_org 与连接归属（peer_org_id）不一致 → 401（防跨连接冒充发起组织）。
#[sqlx::test]
async fn test_federation_call_with_mismatched_caller_org_is_401(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "misma").await;
    let (org_b, _jwt_b) = create_node(&app, "mismb").await;
    let (org_c, _jwt_c) = create_node(&app, "mismc").await;
    seed_gateway_agent(&org_b).await;
    let credential = "f".repeat(64);
    seed_link(&org_b, &org_a, &credential, r#"["a2a_task"]"#).await;

    // A 的合法凭证，但声明冒充 C 组织发起
    let declaration = serde_json::json!({ "caller_org": org_c }).to_string();
    let (status, _body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&credential, Some(&declaration)),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// 无任何凭证（无 JWT 无 Bearer）→ 401。
#[sqlx::test]
async fn test_a2a_without_any_credential_is_401(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (status, _body) = app
        .post_with_headers("/a2a", HeaderMap::new(), &send_task_rpc())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ==================== P3：能力发现 + 连接级白名单 ====================

/// 能力发现端点：契约凭证鉴权，返回连接白名单 + 本节点可调用 Agent 列表。
#[sqlx::test]
async fn test_capabilities_endpoint_returns_agents_and_whitelist(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "capa").await;
    let (org_b, _jwt_b) = create_node(&app, "capb").await;
    seed_gateway_agent(&org_b).await;
    let credential = "1".repeat(64);
    seed_link(&org_b, &org_a, &credential, r#"["a2a_task"]"#).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", credential)).expect("valid bearer"),
    );
    let (status, body) = app
        .get_with_headers("/api/v1/organization/links/capabilities", headers)
        .await;
    assert_eq!(status, StatusCode::OK, "body: {}", body);

    let data = crate::common::assert_api_ok(status, &body);
    // 白名单来自连接的 capabilities 列
    let caps = data
        .get("capabilities")
        .and_then(|v| v.as_array())
        .expect("capabilities array");
    assert!(
        caps.iter().any(|c| c.as_str() == Some("a2a_task")),
        "a2a_task should be in capabilities, got: {}",
        body
    );
    // Agent 列表包含播种的 Onboarded 网关 Agent
    let agents = data
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents array");
    assert!(
        !agents.is_empty(),
        "onboarded gateway agent should be exposed, got: {}",
        body
    );
    assert!(
        agents
            .iter()
            .all(|a| a.get("id").is_some() && a.get("name").is_some()),
        "agent entries should carry id/name, got: {}",
        body
    );
}

/// 白名单门禁：连接 capabilities 不含 a2a_task → /a2a 403（凭证本身有效）。
#[sqlx::test]
async fn test_a2a_rejected_403_when_capability_not_in_whitelist(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "gata").await;
    let (org_b, _jwt_b) = create_node(&app, "gatb").await;
    seed_gateway_agent(&org_b).await;
    let credential = "2".repeat(64);
    // 连接有效但白名单不含 a2a_task
    seed_link(&org_b, &org_a, &credential, r#"["other_cap"]"#).await;

    let (status, body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&credential, None),
            &send_task_rpc(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {}", body);
}
