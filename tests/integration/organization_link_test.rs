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

/// 全链路：issue（B）→ create_link（A，真实 TCP 出站）→ 双方落库 + 凭证交叉校验。
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
        .find_by_pair(ctx_dao.clone(), &org_a_id, &org_b_id)
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
        .find_by_pair(ctx_dao.clone(), &org_b_id, &org_a_id)
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
        .get_by_id(ctx_dao.clone(), &org_a_id)
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

/// S5 目录同步：契约凭证鉴权 + 目录拉取/推送 + 影子 upsert 幂等与 Local 保护。
///
/// 链路：建联（B 签发配对码 → A create_link）→ A 持出站凭证调 B 的
/// GET /directory（真实 TCP）→ 断言目录含双方 → POST /directory/sync 推送
/// 合成条目（独立 uuid）→ 断言 Remote 影子创建 + 新者胜 + Local 保护。
#[sqlx::test]
async fn test_directory_sync_with_credential_auth(pool: SqlitePool) {
    use ::common::api::{DirectorySyncRequest, PeerOrgDirectoryEntry};
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
            &IssuePairingCodeRequest {},
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

    // 取 A 的出站凭证（= B 为 A 生成的 peer_token）
    let link_dao = ai_orz::service::dao::organization_link::dao();
    let ctx_dao = RequestContext::from_storage(
        "federation-test-dir-assert",
        ai_orz::pkg::storage::get().clone(),
    );
    let link_a = link_dao
        .find_by_pair(ctx_dao.clone(), &org_a_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should exist");

    // ---- GET /directory：正确凭证 → 200 且目录含双方组织 ----
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/v1/organization/links/directory",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
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
    // 白名单字段红线：条目绝不携带凭证/业务数据字段
    let first = &orgs[0];
    assert!(
        first.get("access_token").is_none() && first.get("peer_token").is_none(),
        "directory entries must not carry credential fields"
    );

    // ---- GET /directory：错误凭证 → 401（防枚举统一错误）----
    let resp = client
        .get(format!(
            "{}/api/v1/organization/links/directory",
            peer_endpoint
        ))
        .bearer_auth("deadbeef".repeat(8))
        .send()
        .await
        .expect("GET directory with bad credential failed");
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
        updated_at,
    };
    let resp = client
        .post(format!(
            "{}/api/v1/organization/links/directory/sync",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
        .json(&DirectorySyncRequest {
            orgs: vec![entry(&shadow_id, "远端影子组织", 1000)],
        })
        .send()
        .await
        .expect("POST directory/sync over TCP failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // 影子落库：scope=Remote
    let shadow = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao.clone(), &shadow_id)
        .await
        .expect("query shadow failed")
        .expect("Remote shadow should exist after sync");
    assert_eq!(shadow.scope, OrganizationScope::Remote);
    assert_eq!(shadow.name, "远端影子组织");

    // 幂等：再推一次（同版本）→ 不重复、无副作用
    let resp = client
        .post(format!(
            "{}/api/v1/organization/links/directory/sync",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
        .json(&DirectorySyncRequest {
            orgs: vec![entry(&shadow_id, "远端影子组织", 1000)],
        })
        .send()
        .await
        .expect("POST directory/sync replay failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // 新者胜：更新版本覆盖元信息，scope 仍为 Remote
    let resp = client
        .post(format!(
            "{}/api/v1/organization/links/directory/sync",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
        .json(&DirectorySyncRequest {
            orgs: vec![entry(&shadow_id, "远端影子组织-新名", 2000)],
        })
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
    let resp = client
        .post(format!(
            "{}/api/v1/organization/links/directory/sync",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
        .json(&DirectorySyncRequest {
            orgs: vec![entry(&org_a_id, "冒名顶替", 9999)],
        })
        .send()
        .await
        .expect("POST directory/sync local-spoof failed");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let org_a = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx_dao, &org_a_id)
        .await
        .expect("query org A failed")
        .expect("org A should exist");
    assert_ne!(
        org_a.name, "冒名顶替",
        "Local org must never be overwritten"
    );
    assert_eq!(org_a.scope, OrganizationScope::Local);

    // ---- POST /directory/sync：错误凭证 → 401 ----
    let resp = client
        .post(format!(
            "{}/api/v1/organization/links/directory/sync",
            peer_endpoint
        ))
        .bearer_auth("deadbeef".repeat(8))
        .json(&DirectorySyncRequest { orgs: vec![] })
        .send()
        .await
        .expect("POST directory/sync with bad credential failed");
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}

/// S6 断联：管理员 DELETE /links/{peer_org_id} → 连接 Revoked，
/// 对端后续调用本节点凭证鉴权 401（惰性感知），本端组织不被降级。
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
            &IssuePairingCodeRequest {},
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

    // A 的出站凭证（调 B 用）
    let link_dao = ai_orz::service::dao::organization_link::dao();
    let ctx_dao = RequestContext::from_storage(
        "federation-test-revoke-assert",
        ai_orz::pkg::storage::get().clone(),
    );
    let link_a = link_dao
        .find_by_pair(ctx_dao.clone(), &org_a_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should exist");

    // 建联基线：A 调 B 的 directory → 200
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/v1/organization/links/directory",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
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

    // 断联后 A 调 B → 401（凭证哈希不再命中任何 Active 连接，惰性感知）
    let resp = client
        .get(format!(
            "{}/api/v1/organization/links/directory",
            peer_endpoint
        ))
        .bearer_auth(&link_a.access_token)
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
