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
//! 覆盖链路（S2 签名鉴权）：B 管理员签发配对码 → A `POST /links`（出站真实 HTTP，
//! 双向交叉签名验证）→ 双方 `organization_links` 落库 + 对端 DID/公钥一致性 →
//! A 持签名头调 B 的 directory / sync → A `GET /links` 列表可见 B。

#[path = "../common/mod.rs"]
mod common;

use ::common::api::{CreateLinkRequest, InitializeSystemRequest, IssuePairingCodeRequest};
use ai_orz::pkg::RequestContext;
use sqlx::SqlitePool;

/// 通过 Domain 层直接创建测试节点组织 + 登录拿 JWT。
///
/// 不走 `bootstrap_system`（其复用链在高并发下会间歇失败回退 `/initialize`，
/// 撞上共享库下其他用例已建 Local 组织的"已初始化"拦截 → 400 TOCTOU，S5
/// 排查确认）。Domain 的 `create_org_and_owner` 是通用方法，直接建组织
/// 不经过 handler 拦截；每个用例的组织均为 uuid 隔离的私有作用域。
async fn create_node(app: &crate::common::TestApp, tag: &str) -> (String, String) {
    let ctx = RequestContext::from_storage(
        format!("federation-{tag}").as_str(),
        ai_orz::pkg::storage::get().clone(),
    );
    let username = format!("{tag}-admin-{}", uuid::Uuid::now_v7());
    let password = format!("{tag}-pw-{}", uuid::Uuid::now_v7());
    let (org_id, _user_id) = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .create_org_and_owner(
            ctx,
            InitializeSystemRequest {
                organization_name: format!("{tag}-Org-{}", uuid::Uuid::now_v7()),
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

/// 组装带联邦签名头的 reqwest 请求（调用方 = caller_org_id，真实 TCP 出站）
async fn signed_reqwest(
    caller_org_id: &str,
    method: &str,
    url: &str,
    body: Option<&serde_json::Value>,
) -> reqwest::RequestBuilder {
    let ida = crate::common::federation::org_federation_identity(caller_org_id).await;
    let path = ai_orz::pkg::url_util::path_and_query(url)
        .expect("test url should have path")
        .to_string();
    let body_bytes = body
        .map(|b| serde_json::to_vec(b).expect("serialize body"))
        .unwrap_or_default();
    let client = ai_orz::pkg::http::presets::outbound()
        .build()
        .expect("client");
    let mut req = match method {
        "GET" => client.get(url),
        "POST" => client.post(url),
        _ => panic!("unsupported test method"),
    };
    for (name, value) in
        crate::common::federation::signature_headers(&ida, method, &path, &body_bytes).iter()
    {
        req = req.header(name, value);
    }
    if body.is_some() {
        req = req
            .header("Content-Type", "application/json")
            .body(body_bytes);
    }
    req
}

/// 全链路：issue（B）→ create_link（A，真实 TCP 出站）→ 双方落库 + DID/公钥一致性。
#[sqlx::test]
async fn test_create_link_dual_node_full_flow(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    // ---- 节点 A / 节点 B：Domain 层直接建组织（规避 /initialize 竞态，见 helper 注释）----
    let (org_a_id, jwt_a) = create_node(&app, "fulla").await;
    let (org_b_id, jwt_b) = create_node(&app, "fullb").await;

    // ---- 节点 B 真实 TCP server（供 A 出站调用）----
    let peer_endpoint = app.serve_on_random_port().await;

    // ---- B 管理员签发配对码 ----
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links/pairing/issue",
            &IssuePairingCodeRequest::default(),
            &jwt_b,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let pairing_code = data
        .get("pairing_code")
        .and_then(|v| v.as_str())
        .expect("pairing_code should exist")
        .to_string();

    // ---- A 发起建联（服务端出站真实 HTTP 调 B verify，双向交叉签名）----
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

    // A → B：endpoint 指向 B 的 TCP 地址，对端身份 = B 的真实 DID / 公钥
    let link_a = link_dao
        .find_by_pair(ctx_dao.clone(), &org_a_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should exist after create_link");
    assert_eq!(link_a.endpoint, peer_endpoint, "A→B endpoint");
    let id_b = crate::common::federation::org_federation_identity(&org_b_id).await;
    assert_eq!(
        link_a.peer_did.as_deref(),
        Some(id_b.did.as_str()),
        "A→B link should carry B's DID"
    );
    assert_eq!(
        link_a.peer_verification_key.as_deref(),
        Some(id_b.verification_key.as_str()),
        "A→B link should carry B's verification key"
    );

    // B → A：endpoint 指向 A 的联邦基址（config 缺省推导 http://127.0.0.1:3000）
    let link_b = link_dao
        .find_by_pair(ctx_dao.clone(), &org_b_id, &org_a_id)
        .await
        .expect("query B→A link failed")
        .expect("B→A link should exist (written by B-side verify)");
    assert_eq!(
        link_b.endpoint, "http://127.0.0.1:3000",
        "B→A endpoint should be A's config-derived federation base URL"
    );

    // ---- 对端身份一致性（S2：DID/公钥由建联交叉签名验证后落库）----
    let id_a = crate::common::federation::org_federation_identity(&org_a_id).await;
    assert_eq!(
        link_b.peer_did.as_deref(),
        Some(id_a.did.as_str()),
        "B→A link should carry A's DID"
    );
    assert_eq!(
        link_b.peer_verification_key.as_deref(),
        Some(id_a.verification_key.as_str()),
        "B→A link should carry A's verification key"
    );

    // ---- R5 Local 保护：共享库下双方组织 scope 仍为 Local（未被对端覆盖）----
    use ::common::enums::OrganizationScope;
    let org_a = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &org_a_id)
        .await
        .expect("query org A failed")
        .expect("org A should exist");
    assert_eq!(
        org_a.scope,
        OrganizationScope::Local,
        "org A must stay Local (upsert_linked_shadow R5 protection)"
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

    // 发起方用 Domain 层直接建组织（uuid 隔离的私有作用域，共享库下无并发写入者）
    let (org_id, jwt) = create_node(&app, "invalid").await;

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

/// S5 目录同步（S2 签名鉴权）：目录拉取/推送 + 影子 upsert 幂等与 Local 保护。
///
/// 链路：建联（B 签发配对码 → A create_link）→ A 持签名头调 B 的
/// GET /directory（真实 TCP）→ 断言目录含双方 → POST /directory/sync 推送
/// 合成条目（独立 uuid）→ 断言 Remote 影子创建 + 新者胜 + Local 保护。
#[sqlx::test]
async fn test_directory_sync_with_signature_auth(pool: SqlitePool) {
    use ::common::api::PeerOrgDirectoryEntry;
    use ::common::enums::OrganizationScope;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    // 节点 A + 节点 B（Domain 层直接建组织，规避 /initialize 竞态）
    let (org_a_id, jwt_a) = create_node(&app, "dira").await;
    let (org_b_id, jwt_b) = create_node(&app, "dirb").await;

    // 建联：B 签发配对码 → A create_link（真实 TCP 出站 + 建联后自动目录双向同步）
    let peer_endpoint = app.serve_on_random_port().await;
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links/pairing/issue",
            &IssuePairingCodeRequest::default(),
            &jwt_b,
        )
        .await;
    let pairing_code = crate::common::assert_api_ok(status, &body)
        .get("pairing_code")
        .and_then(|v| v.as_str())
        .expect("pairing_code should exist")
        .to_string();
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links",
            &CreateLinkRequest {
                pairing_code,
                peer_endpoint: peer_endpoint.clone(),
            },
            &jwt_a,
        )
        .await;
    crate::common::assert_api_ok(status, &body);

    let directory_url = format!("{}/api/v1/organization/links/directory", peer_endpoint);
    let sync_url = format!("{}/api/v1/organization/links/directory/sync", peer_endpoint);

    // ---- GET /directory：正确签名 → 200 且目录含双方组织 ----
    let resp = signed_reqwest(&org_a_id, "GET", &directory_url, None)
        .await
        .send()
        .await
        .expect("GET directory over TCP failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let api: serde_json::Value = resp.json().await.expect("directory response JSON");
    let orgs = api
        .get("data")
        .and_then(|d| d.get("orgs"))
        .and_then(|v| v.as_array())
        .expect("data.orgs array");
    let ids: Vec<&str> = orgs
        .iter()
        .filter_map(|o| o.get("id").and_then(|v| v.as_str()))
        .collect();
    assert!(
        ids.contains(&org_b_id.as_str()) && ids.contains(&org_a_id.as_str()),
        "directory should contain both orgs, got: {:?}",
        ids
    );
    // 白名单字段红线：条目绝不携带凭证/私钥字段
    let first = &orgs[0];
    assert!(
        first.get("access_token").is_none()
            && first.get("peer_token").is_none()
            && first.get("signing_key").is_none(),
        "directory entries must not carry credential fields"
    );

    // ---- GET /directory：未知密钥对签名 → 401（防枚举统一错误）----
    let mut bad_headers = crate::common::federation::unknown_keypair_headers(
        "GET",
        "/api/v1/organization/links/directory",
        b"",
    );
    let resp = ai_orz::pkg::http::presets::outbound()
        .build()
        .expect("client")
        .get(&directory_url)
        .headers(std::mem::take(&mut bad_headers))
        .send()
        .await
        .expect("GET directory with unknown keypair failed");
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);

    // ---- POST /directory/sync：推送合成条目 → Remote 影子创建 ----
    let shadow_id = format!("shadow-{}", uuid::Uuid::now_v7());
    let entry = |id: &str, name: &str, updated_at: i64| PeerOrgDirectoryEntry {
        id: id.to_string(),
        name: name.to_string(),
        description: "synced from peer".to_string(),
        base_url: "https://peer.example.com".to_string(),
        group_name: Some("同步集团".to_string()),
        status: 1,
        did: Some("did:key:z6MkPeerTest".to_string()),
        verification_key: Some("cGVlci1rZXk".to_string()),
        updated_at,
        addresses: None,
    };
    let resp = signed_reqwest(
        &org_a_id,
        "POST",
        &sync_url,
        Some(&serde_json::json!({ "orgs": [entry(&shadow_id, "远端影子组织", 1000)] })),
    )
    .await
    .send()
    .await
    .expect("POST directory/sync over TCP failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // 影子落库：scope=Remote
    let ctx_dao = RequestContext::from_storage(
        "federation-test-dir-assert",
        ai_orz::pkg::storage::get().clone(),
    );
    let shadow = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &shadow_id)
        .await
        .expect("query shadow failed")
        .expect("Remote shadow should exist after sync");
    assert_eq!(shadow.scope, OrganizationScope::Remote);
    assert_eq!(shadow.name, "远端影子组织");

    // 新者胜：更新版本覆盖元信息，scope 仍为 Remote
    let resp = signed_reqwest(
        &org_a_id,
        "POST",
        &sync_url,
        Some(&serde_json::json!({ "orgs": [entry(&shadow_id, "远端影子组织-新名", 2000)] })),
    )
    .await
    .send()
    .await
    .expect("POST directory/sync newer failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let shadow = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &shadow_id)
        .await
        .expect("query shadow failed")
        .expect("shadow should exist");
    assert_eq!(shadow.name, "远端影子组织-新名");
    assert_eq!(shadow.scope, OrganizationScope::Remote);

    // Local 保护：推送中伪造本端组织 id → 不覆盖
    let resp = signed_reqwest(
        &org_a_id,
        "POST",
        &sync_url,
        Some(&serde_json::json!({ "orgs": [entry(&org_a_id, "冒名顶替", 9999)] })),
    )
    .await
    .send()
    .await
    .expect("POST directory/sync local-spoof failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let org_a = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &org_a_id)
        .await
        .expect("query org A failed")
        .expect("org A should exist");
    assert_ne!(
        org_a.name, "冒名顶替",
        "Local org must never be overwritten"
    );
    assert_eq!(org_a.scope, OrganizationScope::Local);

    // ---- POST /directory/sync：未知密钥对 → 401 ----
    let mut bad_headers = crate::common::federation::unknown_keypair_headers(
        "POST",
        "/api/v1/organization/links/directory/sync",
        br#"{"orgs":[]}"#,
    );
    let resp = ai_orz::pkg::http::presets::outbound()
        .build()
        .expect("client")
        .post(&sync_url)
        .headers(std::mem::take(&mut bad_headers))
        .header("Content-Type", "application/json")
        .body(br#"{"orgs":[]}"#.to_vec())
        .send()
        .await
        .expect("POST directory/sync with unknown keypair failed");
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}

/// S6 断联：管理员 DELETE /links/{peer_org_id} → 连接 Revoked，
/// 对端后续调用本节点验签 401（key_id 不再命中 Active 连接，惰性感知），
/// 本端组织不被降级。
#[sqlx::test]
async fn test_revoke_link_blocks_peer_calls(pool: SqlitePool) {
    use ::common::enums::OrganizationScope;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    // 节点 A + 节点 B（Domain 层直接建组织，规避 /initialize 竞态）
    let (org_a_id, jwt_a) = create_node(&app, "revokea").await;
    let (org_b_id, jwt_b) = create_node(&app, "revokeb").await;

    let peer_endpoint = app.serve_on_random_port().await;
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links/pairing/issue",
            &IssuePairingCodeRequest::default(),
            &jwt_b,
        )
        .await;
    let pairing_code = crate::common::assert_api_ok(status, &body)
        .get("pairing_code")
        .and_then(|v| v.as_str())
        .expect("pairing_code should exist")
        .to_string();
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links",
            &CreateLinkRequest {
                pairing_code,
                peer_endpoint: peer_endpoint.clone(),
            },
            &jwt_a,
        )
        .await;
    crate::common::assert_api_ok(status, &body);

    let link_dao = ai_orz::service::dao::organization_link::dao();
    let ctx_dao = RequestContext::from_storage(
        "federation-test-revoke-assert",
        ai_orz::pkg::storage::get().clone(),
    );

    // 建联基线：A 调 B 的 directory → 200
    let directory_url = format!("{}/api/v1/organization/links/directory", peer_endpoint);
    let resp = signed_reqwest(&org_a_id, "GET", &directory_url, None)
        .await
        .send()
        .await
        .expect("GET directory baseline failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // ---- B 管理员断联（撤掉 B→A 连接）----
    let (status, body) = app
        .delete_with_jwt(&format!("/api/v1/organization/links/{}", org_a_id), &jwt_b)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "revoke response should confirm success"
    );

    // B→A 连接已 Revoked
    let link_b = link_dao
        .find_by_pair(ctx_dao.clone(), &org_b_id, &org_a_id)
        .await
        .expect("query B→A link failed")
        .expect("B→A link row should remain (审计线索不删除)");
    assert_eq!(
        link_b.status.to_i32(),
        0,
        "B→A link should be Revoked after DELETE"
    );

    // 断联后 A 调 B → 401（A 的 DID 不再命中 B 侧任何 Active 连接，惰性感知）
    let resp = signed_reqwest(&org_a_id, "GET", &directory_url, None)
        .await
        .send()
        .await
        .expect("GET directory after revoke failed");
    assert_eq!(
        resp.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "peer calls must 401 after revoke"
    );

    // 共享库下 A 的组织 scope 仍为 Local（真实部署中 B 侧影子 Linked→Remote；
    // R5 保护：本地组织绝不降级）
    let org_a = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &org_a_id)
        .await
        .expect("query org A failed")
        .expect("org A should exist");
    assert_eq!(org_a.scope, OrganizationScope::Local);

    // 断联不删除记录：A→B 连接不受影响仍 Active（有向契约，B 只撤自己的出边）
    let link_a_after = link_dao
        .find_by_pair(ctx_dao, &org_a_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should remain untouched");
    assert_eq!(link_a_after.status.to_i32(), 1);
}
