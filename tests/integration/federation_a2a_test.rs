//! 联邦调用鉴权集成测试（S2 签名鉴权验收）。
//!
//! `/a2a` 双模鉴权：
//! - 本地 JWT（既有语义，a2a_flow_test 已覆盖，不在此重复）；
//! - 建联对端节点：每请求 Ed25519 签名（`X-Federation-Key-Id / Timestamp /
//!   Nonce / Signature` 四头，签名串 = `method\npath\ntimestamp\nnonce\nsha256(body)`，
//!   验签用的对端公钥在建联时交换落库）+ 可选 `X-Federation-Caller` 身份声明。
//!
//! 覆盖：有效签名 + 声明、无声明（两者 ctx.user_id 均映射到 B 侧**接待用户**，
//! P6：声明仅作审计/计量，不再决定内部身份）、未知 DID 签名 401、非法声明 401
//! （fail-closed）、声明组织与连接归属不一致 401（防跨连接冒充）、无任何签名头 401、
//! 能力白名单 403、能力发现端点。
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
/// peer_did / peer_verification_key = org_a 的真实联邦身份（B 据此验 A 的签名）。
async fn seed_link(org_b: &str, org_a: &str, capabilities: &str) {
    let ctx = RequestContext::from_storage("fed-a2a-seed", ai_orz::pkg::storage::get().clone());
    let ida = crate::common::federation::org_federation_identity(org_a).await;
    let mut link = OrganizationLinkPo::new(
        Uuid::now_v7().to_string(),
        org_b.to_string(),
        org_a.to_string(),
        "https://peer-a.example.com".to_string(),
        "fed-a2a-seed".to_string(),
    );
    link.peer_did = Some(ida.did);
    link.peer_verification_key = Some(ida.verification_key);
    ai_orz::service::dao::organization_link::dao()
        .insert(ctx.clone(), &link)
        .await
        .expect("seed link failed");

    // S3：能力白名单在合约（federation_contracts），按测试参数覆盖默认值
    let mut contract = ai_orz::models::federation_contract::FederationContractPo::new(
        org_b.to_string(),
        org_a.to_string(),
    );
    contract.capabilities = capabilities.to_string();
    ai_orz::service::dao::federation_contract::dao()
        .insert(ctx, &contract)
        .await
        .expect("seed contract failed");
}

/// 组装联邦签名调用请求 headers（/a2a POST，body 为序列化后的 JSON-RPC）
fn federation_headers(
    ida: &crate::common::federation::OrgFederationIdentity,
    body: &serde_json::Value,
    declaration: Option<&String>,
) -> HeaderMap {
    let body_bytes = serde_json::to_vec(body).expect("serialize rpc");
    let mut headers =
        crate::common::federation::signature_headers(ida, "POST", "/a2a", &body_bytes);
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

/// 有效签名 + 完整声明：鉴权通过，ctx.user_id = B 侧接待用户（P6：
/// 内部身份由被访组织决定，声明仅作审计/计量），
/// project 落在目标组织 B（organization_id = B，caller org = A 走日志维度）。
#[sqlx::test]
async fn test_federation_call_with_declaration_creates_task(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "feda").await;
    let (org_b, _jwt_b) = create_node(&app, "fedb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let declaration = serde_json::json!({
        "caller_org": org_a,
        "caller_user": format!("user-a-{}", Uuid::now_v7()),
        "caller_agent": format!("agent-a-{}", Uuid::now_v7()),
    })
    .to_string();

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&ida, &rpc, Some(&declaration)),
            &rpc,
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

    // ctx 注入断言：created_by = B 侧接待用户（P6 接待用户映射；声明不再决定内部身份）
    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let project = ai_orz::service::dao::project::dao()
        .find_by_id(ctx.clone(), &task_id)
        .await
        .expect("query project failed")
        .expect("project should exist for a2a task");
    let reception = ai_orz::service::dao::user::dao()
        .find_reception_user(ctx, &org_b)
        .await
        .expect("query reception user")
        .expect("org_b should have an admin as reception user");
    assert_eq!(
        project.created_by, reception.id,
        "ctx.user_id should map to B-side reception user (org admin)"
    );
    // 来源审计断言：联邦请求的 project tags 应含 federation:<对端org>
    let tags = project.get_tags();
    assert!(
        tags.contains(&format!("federation:{}", org_a)),
        "project tags should record source org, got: {:?}",
        tags
    );
    assert!(
        tags.iter().any(|t| t == "a2a"),
        "tags should keep base a2a marker"
    );
}

/// 有效签名 + 无声明：连接级调用，内部身份同样映射到 B 侧接待用户（P6）。
#[sqlx::test]
async fn test_federation_call_without_declaration_uses_reception_user(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "anona").await;
    let (org_b, _jwt_b) = create_node(&app, "anonb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
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
        .find_by_id(ctx.clone(), &task_id)
        .await
        .expect("query project failed")
        .expect("project should exist");
    let reception = ai_orz::service::dao::user::dao()
        .find_reception_user(ctx, &org_b)
        .await
        .expect("query reception user")
        .expect("org_b should have an admin as reception user");
    assert_eq!(
        project.created_by, reception.id,
        "anonymous connection-level call also maps to reception user (P6)"
    );
}

/// 签名者 DID 不归属任何 Active 连接（未知密钥对）→ 401（防枚举，统一响应）。
#[sqlx::test]
async fn test_federation_call_with_unknown_keypair_is_401(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "wronga").await;
    let (org_b, _jwt_b) = create_node(&app, "wrongb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let rpc = send_task_rpc();
    let mut headers = crate::common::federation::unknown_keypair_headers(
        "POST",
        "/a2a",
        &serde_json::to_vec(&rpc).unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let (status, body) = app.post_with_headers("/a2a", headers, &rpc).await;
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
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let bad_declaration = "not-json".to_string();
    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, _body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&ida, &rpc, Some(&bad_declaration)),
            &rpc,
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
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    // A 的合法签名，但声明冒充 C 组织发起
    let declaration = serde_json::json!({ "caller_org": org_c }).to_string();
    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, _body) = app
        .post_with_headers(
            "/a2a",
            federation_headers(&ida, &rpc, Some(&declaration)),
            &rpc,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// 无任何签名头（无 JWT 无签名）→ 401。
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

/// 能力发现端点：联邦签名鉴权，返回连接白名单 + 本节点可调用 Agent 列表。
#[sqlx::test]
async fn test_capabilities_endpoint_returns_agents_and_whitelist(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "capa").await;
    let (org_b, _jwt_b) = create_node(&app, "capb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let headers = crate::common::federation::signature_headers(
        &ida,
        "GET",
        "/api/v1/organization/links/capabilities",
        b"",
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

/// 白名单门禁：连接 capabilities 不含 a2a_task → /a2a 403（签名本身有效）。
#[sqlx::test]
async fn test_a2a_rejected_403_when_capability_not_in_whitelist(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "gata").await;
    let (org_b, _jwt_b) = create_node(&app, "gatb").await;
    seed_gateway_agent(&org_b).await;
    // 连接有效但白名单不含 a2a_task
    seed_link(&org_b, &org_a, r#"["other_cap"]"#).await;

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {}", body);
}

// ==================== S3 合约授权验收 ====================
//
// 能力集由 federation_contracts 派生（唯一事实源）：无 active 合约 = 403
// （fail-closed）；terminated = 403；管理员改能力集，下一次请求即生效。

use ::common::enums::FederationContractState as TestContractState;

/// S3：连接存在但无任何合约记录 → 403（fail-closed，无合约 = 无能力）。
#[sqlx::test]
async fn test_a2a_rejected_403_when_no_contract(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "nocta").await;
    let (org_b, _jwt_b) = create_node(&app, "noctb").await;
    seed_gateway_agent(&org_b).await;

    // 只播种 link，不播种合约
    let ida0 = crate::common::federation::org_federation_identity(&org_a).await;
    let ctx = RequestContext::from_storage("fed-a2a-seed", ai_orz::pkg::storage::get().clone());
    let mut link = OrganizationLinkPo::new(
        Uuid::now_v7().to_string(),
        org_b.clone(),
        org_a.clone(),
        "https://peer-a.example.com".to_string(),
        "fed-a2a-seed".to_string(),
    );
    link.peer_did = Some(ida0.did);
    link.peer_verification_key = Some(ida0.verification_key);
    ai_orz::service::dao::organization_link::dao()
        .insert(ctx, &link)
        .await
        .expect("seed link failed");

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "no contract => no capability (fail-closed), body: {}",
        body
    );
}

/// S3：合约 terminated → 403（fail-closed）。
#[sqlx::test]
async fn test_a2a_rejected_403_when_contract_terminated(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "terma").await;
    let (org_b, _jwt_b) = create_node(&app, "termb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    // 终止合约
    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let mut contract = ai_orz::service::dao::federation_contract::dao()
        .find_by_pair(ctx.clone(), &org_b, &org_a)
        .await
        .expect("query contract failed")
        .expect("contract should exist after seed");
    contract.state = TestContractState::Terminated;
    ai_orz::service::dao::federation_contract::dao()
        .update(ctx, &contract)
        .await
        .expect("terminate contract failed");

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "terminated contract => rejected, body: {}",
        body
    );
}

/// S3：管理员编辑合约能力集后，下一次请求即按新能力集判定。
#[sqlx::test]
async fn test_admin_updates_contract_capabilities_takes_effect(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "edita").await;
    let (org_b, jwt_b) = create_node(&app, "editb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"[]"#).await;

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();

    // 初始：合约能力为空 → 403
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {}", body);

    // 管理员（B 侧）编辑合约能力集 → 开放 a2a_task
    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let contract = ai_orz::service::dao::federation_contract::dao()
        .find_by_pair(ctx.clone(), &org_b, &org_a)
        .await
        .expect("query contract failed")
        .expect("contract should exist");
    let (status, body) = app
        .put_with_jwt(
            "/api/v1/organization/contracts/capabilities",
            &serde_json::json!({
                "contract_id": contract.id,
                "capabilities": ["a2a_task"],
            }),
            &jwt_b,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "update capabilities should succeed, body: {}",
        body
    );

    // 下一次请求即按新能力集判定 → 200
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "updated capabilities should take effect immediately, body: {}",
        body
    );
}

/// S3：未知能力拒绝（400，防拼写错误静默扩权）；terminate 端点生效。
#[sqlx::test]
async fn test_contract_admin_endpoints_validate_and_terminate(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, _jwt_a) = create_node(&app, "adma").await;
    let (org_b, jwt_b) = create_node(&app, "admb").await;
    seed_gateway_agent(&org_b).await;
    seed_link(&org_b, &org_a, r#"["a2a_task"]"#).await;

    let ctx = RequestContext::from_storage("fed-a2a-assert", ai_orz::pkg::storage::get().clone());
    let contract = ai_orz::service::dao::federation_contract::dao()
        .find_by_pair(ctx.clone(), &org_b, &org_a)
        .await
        .expect("query contract failed")
        .expect("contract should exist");

    // 未知能力 → 400
    let (status, _body) = app
        .put_with_jwt(
            "/api/v1/organization/contracts/capabilities",
            &serde_json::json!({
                "contract_id": contract.id,
                "capabilities": ["make_coffee"],
            }),
            &jwt_b,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::BAD_REQUEST,
        "unknown capability should be rejected"
    );

    // terminate → 200；随后请求 403
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/contracts/terminate",
            &serde_json::json!({ "peer_org_id": org_a }),
            &jwt_b,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "terminate should succeed, body: {}",
        body
    );

    let ida = crate::common::federation::org_federation_identity(&org_a).await;
    let rpc = send_task_rpc();
    let (status, body) = app
        .post_with_headers("/a2a", federation_headers(&ida, &rpc, None), &rpc)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "terminated contract => rejected, body: {}",
        body
    );

    // 合约记录保留（审计线索），state = terminated
    let terminated = ai_orz::service::dao::federation_contract::dao()
        .find_by_pair(ctx, &org_b, &org_a)
        .await
        .expect("query contract failed")
        .expect("contract record should be kept for audit");
    assert_eq!(terminated.state, TestContractState::Terminated);
}
