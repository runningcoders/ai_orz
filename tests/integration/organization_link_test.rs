//! 组织组网建联全链路集成测试（评审稿 S4 验收：双 server 实例完成建联）。
//!
//! 双节点模拟约束：所有集成测试共享同一全局 Storage 单例（`storage::init`
//! 用 `OnceLock`，第二次调用 no-op），无法真正隔离两套 DB。因此：
//! - 节点 A = in-process `TestApp`（`oneshot`）；
//! - 节点 B = 同进程真实 TCP server（`TestApp::serve_on_random_port`），
//!   供节点 A 的 Domain 层 reqwest 出站调对端 verify（`oneshot` 无法覆盖出站 HTTP）；
//! - 双方组织同库均为 `scope=Local`，对端影子 upsert 会命中 R5 的 Local 保护
//!   而跳过——这本身是断言之一（本端组织绝不被对端覆盖）。
//!
//! 覆盖链路：B 管理员签发配对码 → A `POST /links`（出站真实 HTTP）→
//! 双方 `organization_links` 落库 + 交叉凭证一致性（A 的 access_token 与 B 的
//! peer_token_hash 互为 sha256 对）→ A `GET /links` 列表可见 B。

#[path = "../common/mod.rs"]
mod common;

use ::common::api::{CreateLinkRequest, InitializeSystemRequest, IssuePairingCodeRequest};
use ai_orz::pkg::RequestContext;
use sqlx::SqlitePool;

/// 全链路：issue（B）→ create_link（A，真实 TCP 出站）→ 双方落库 + 凭证交叉校验。
#[sqlx::test]
async fn test_create_link_dual_node_full_flow(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    // ---- 节点 A：bootstrap（复用/新建 Local 组织 + SuperAdmin）----
    let bs_a = crate::common::factories::bootstrap_system(&app).await;
    let jwt_a = crate::common::factories::login_and_get_jwt(
        &app,
        &bs_a.organization_id,
        &bs_a.username,
        &bs_a.password,
    )
    .await;

    // ---- 节点 B：Domain 层直接建第二个组织（handler 层有"已初始化"拦截，
    //      共享库下系统只允许一个 Local 组织；Domain 的 create_org_and_owner
    //      是通用方法，绕过 handler 拦截模拟第二个节点）----
    let ctx_sys =
        RequestContext::from_storage("federation-test-b", ai_orz::pkg::storage::get().clone());
    let username_b = format!("peer-admin-{}", uuid::Uuid::now_v7());
    let password_b = format!("peer-pw-{}", uuid::Uuid::now_v7());
    let (org_b_id, _user_b_id) = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .create_org_and_owner(
            ctx_sys.clone(),
            InitializeSystemRequest {
                organization_name: format!("PeerOrg-{}", uuid::Uuid::now_v7()),
                admin_username: username_b.clone(),
                admin_password: password_b.clone(),
                description: Some("Federation peer node B".to_string()),
                admin_display_name: Some("Peer Admin".to_string()),
                admin_email: None,
                chat_model: None,
                embedding_model: None,
            },
        )
        .await
        .expect("create peer org B should succeed");
    let jwt_b =
        crate::common::factories::login_and_get_jwt(&app, &org_b_id, &username_b, &password_b)
            .await;

    // ---- 节点 B 真实 TCP server（供 A 出站调用）----
    let peer_endpoint = app.serve_on_random_port().await;

    // ---- B 管理员签发配对码 ----
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links/pairing/issue",
            &IssuePairingCodeRequest {},
            &jwt_b,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let pairing_code = data
        .get("pairing_code")
        .and_then(|v| v.as_str())
        .expect("pairing_code should exist")
        .to_string();

    // ---- A 发起建联（服务端出站真实 HTTP 调 B verify）----
    let req = CreateLinkRequest {
        pairing_code,
        peer_endpoint: peer_endpoint.clone(),
    };
    let (status, body) = app
        .post_with_jwt("/api/v1/organization/links", &req, &jwt_a)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "create_link should succeed, got body: {}",
        body
    );
    let data = crate::common::assert_api_ok(status, &body);
    let link = data.get("link").expect("link should exist in response");
    assert_eq!(
        link.get("peer_org")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str()),
        Some(org_b_id.as_str()),
        "response peer_org.id should be org B"
    );
    assert_eq!(
        link.get("endpoint").and_then(|v| v.as_str()),
        Some(peer_endpoint.as_str()),
        "response endpoint should echo peer_endpoint"
    );
    assert_eq!(
        link.get("status").and_then(|v| v.as_i64()),
        Some(1),
        "link status should be Active(1)"
    );

    // ---- 双方 organization_links 落库断言 ----
    let link_dao = ai_orz::service::dao::organization_link::dao();
    let ctx_dao = RequestContext::from_storage(
        "federation-test-assert",
        ai_orz::pkg::storage::get().clone(),
    );

    // A → B：endpoint 指向 B 的 TCP 地址，凭证均为 64 hex
    let link_a = link_dao
        .find_by_pair(ctx_dao.clone(), &bs_a.organization_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should exist after create_link");
    assert_eq!(link_a.endpoint, peer_endpoint, "A→B endpoint");
    assert_eq!(
        link_a.access_token.len(),
        64,
        "A→B access_token should be 64-hex"
    );
    assert_eq!(
        link_a.peer_token_hash.len(),
        64,
        "A→B peer_token_hash should be 64-hex"
    );

    // B → A：endpoint 指向 A 的联邦基址（config 缺省推导 http://127.0.0.1:3000）
    let link_b = link_dao
        .find_by_pair(ctx_dao.clone(), &org_b_id, &bs_a.organization_id)
        .await
        .expect("query B→A link failed")
        .expect("B→A link should exist (written by B-side verify)");
    assert_eq!(
        link_b.endpoint, "http://127.0.0.1:3000",
        "B→A endpoint should be A's config-derived federation base URL"
    );

    // ---- 交叉凭证一致性（D6 双向独立凭证）----
    // B 的 access_token（= A 生成的 local_token 明文）的 sha256 == A 的 peer_token_hash
    assert_eq!(
        sha256::digest(link_b.access_token.as_bytes()),
        link_a.peer_token_hash,
        "B's outbound credential must match A's inbound hash"
    );
    // A 的 access_token（= B 生成的 peer_token 明文）的 sha256 == B 的 peer_token_hash
    assert_eq!(
        sha256::digest(link_a.access_token.as_bytes()),
        link_b.peer_token_hash,
        "A's outbound credential must match B's inbound hash"
    );
    // 双向凭证独立（不同 token）
    assert_ne!(
        link_a.access_token, link_b.access_token,
        "bidirectional tokens must be independent (D6)"
    );

    // ---- R5 Local 保护：共享库下双方组织 scope 仍为 Local（未被对端覆盖）----
    use ::common::enums::OrganizationScope;
    let org_a = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &bs_a.organization_id)
        .await
        .expect("query org A failed")
        .expect("org A should exist");
    assert_eq!(
        org_a.scope,
        OrganizationScope::Local,
        "org A must stay Local (upsert_linked_peer_org R5 protection)"
    );

    // ---- A 的已建联列表可见 B ----
    let (status, body) = app.get_with_jwt("/api/v1/organization/links", &jwt_a).await;
    assert_eq!(status, axum::http::StatusCode::OK, "list_links: {}", body);
    let data = crate::common::assert_api_ok(status, &body);
    let links = data
        .get("links")
        .and_then(|v| v.as_array())
        .expect("links array should exist");
    assert!(
        links.iter().any(|l| l
            .get("peer_org")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            == Some(org_b_id.as_str())),
        "org B should appear in A's link list, got: {}",
        body
    );
}

/// 配对码无效（不存在/过期/已用）→ 建联失败且本地不落任何 link。
#[sqlx::test]
async fn test_create_link_rejects_invalid_pairing_code(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    // 先走 bootstrap_system（BOOTSTRAP_MUTEX 串行化）：保证首个 Local 组织由
    // /initialize 或复用路径唯一产出，避免与全链路用例的 /initialize 竞态
    // （共享库下"已初始化"拦截 → 400）。
    let _bs = crate::common::factories::bootstrap_system(&app).await;

    // 发起方用独立组织（bootstrap_system 复用共享 Local 组织，其 link 集
    // 会被同二进制的全链路用例并发写入，不能作为"无 link"断言的私有作用域）
    let ctx_sys = RequestContext::from_storage(
        "federation-test-invalid-init",
        ai_orz::pkg::storage::get().clone(),
    );
    let username = format!("invalid-admin-{}", uuid::Uuid::now_v7());
    let password = format!("invalid-pw-{}", uuid::Uuid::now_v7());
    let (org_id, _user_id) = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .create_org_and_owner(
            ctx_sys,
            InitializeSystemRequest {
                organization_name: format!("InvalidOrg-{}", uuid::Uuid::now_v7()),
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
        .expect("create initiator org should succeed");
    let jwt =
        crate::common::factories::login_and_get_jwt(&app, &org_id, &username, &password).await;

    // 对端在线但配对码无效
    let peer_endpoint = app.serve_on_random_port().await;

    let req = CreateLinkRequest {
        pairing_code: "BADCODEBADCODEBADCODEBAD".to_string(), // 24 字符但不存在于对端
        peer_endpoint,
    };
    let (status, body) = app
        .post_with_jwt("/api/v1/organization/links", &req, &jwt)
        .await;
    assert_ne!(
        status,
        axum::http::StatusCode::OK,
        "invalid pairing code must not create a link, got body: {}",
        body
    );

    // 本端不落 link（org_id 为本用例私有，共享库下无并发写入者）
    let link_dao = ai_orz::service::dao::organization_link::dao();
    let ctx_dao = RequestContext::from_storage(
        "federation-test-invalid-assert",
        ai_orz::pkg::storage::get().clone(),
    );
    let links = link_dao
        .query(
            ctx_dao,
            ai_orz::service::dao::organization_link::OrganizationLinkQuery {
                local_org_id: Some(org_id),
                status: None,
                limit: Some(50),
            },
        )
        .await
        .expect("query links failed");
    assert!(
        links.is_empty(),
        "no link should be persisted on failed handshake, got: {:?}",
        links
    );
}
