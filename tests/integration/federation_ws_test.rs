//! P8 联邦 WS 长连接 e2e（阶段 3 收尾验证）。
//!
//! 双节点模拟约束与 `organization_link_test` 相同：共享同一全局 Storage，
//! 节点 B = 真实 TCP server（`serve_on_random_port`），节点 A = in-process
//! 出站（WS client 拨真实 TCP）。
//!
//! 覆盖链路（P8 最小闭环）：
//! 1. 建联（真实 HTTP pairing/verify）→ 双方 link 落库 + 凭证交叉一致；
//! 2. A 拨 B 的 WS 端点（Bearer = link.access_token，B 侧握手鉴权 +
//!    能力门禁 + 接待用户映射）→ B 端会话注册（反向可达）；
//! 3. A `request_over_ws` 发 send_task 命令（params 非法）→ 帧出站 →
//!    B 收帧 publish AOP 入站事件 → `FederationInboundTaskConsumer`
//!    （Async）解析失败 → publish 出站事件 → B 端出站 consumer 沿同一
//!    连接推回 → A 的 pending 表唤醒 → 断言错误响应往返成功。
//!
//! 用「非法参数」验证全链路：无需 seed 网关 Agent / project，即可覆盖
//! 帧信封、握手鉴权、事件分发、响应配对四段链路。

#[path = "../common/mod.rs"]
mod common;

use ::common::api::{CreateLinkRequest, InitializeSystemRequest, IssuePairingCodeRequest};
use ai_orz::models::events::FEDERATION_CMD_SEND_TASK;
use ai_orz::pkg::RequestContext;
use ai_orz::service::dao::organization_link::ws;
use serde_json::json;
use sqlx::SqlitePool;
use std::time::Duration;

/// 通过 Domain 层直接创建测试节点组织 + 登录拿 JWT（与 organization_link_test
/// 同款：规避 /initialize 竞态，见该文件 helper 注释）。
async fn create_node(app: &common::TestApp, tag: &str) -> (String, String) {
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
    let jwt = common::factories::login_and_get_jwt(app, &org_id, &username, &password).await;
    (org_id, jwt)
}

/// ws_url_from_base 纯函数：base / 带 /a2a 后缀 / 尾斜杠 / scheme 转换
#[test]
fn ws_url_derivation() {
    assert_eq!(
        ws::ws_url_from_base("http://127.0.0.1:3000"),
        "ws://127.0.0.1:3000/api/v1/organization/links/ws"
    );
    assert_eq!(
        ws::ws_url_from_base("http://127.0.0.1:3000/a2a"),
        "ws://127.0.0.1:3000/api/v1/organization/links/ws"
    );
    assert_eq!(
        ws::ws_url_from_base("https://fed.example.com/"),
        "wss://fed.example.com/api/v1/organization/links/ws"
    );
}

/// e2e：建联 → A 拨 B → send_task 命令往返（非法参数 → 错误响应帧回推）
#[sqlx::test]
async fn test_federation_ws_command_roundtrip(pool: SqlitePool) {
    let _ = common::init_full_test_env(pool.clone()).await;
    // 启动 AOP 调度器（Async consumer worker）：真实运行由 aop::init_all 启动，
    // 测试 env 只注册不启动；本测试进程独立，启动不影响其他用例
    ai_orz::pkg::aop::registry()
        .start_all()
        .await
        .expect("start aop workers");
    let app = common::TestApp::new(pool).await;

    // ---- 节点 A / B ----
    let (org_a_id, jwt_a) = create_node(&app, "wsa").await;
    let (org_b_id, jwt_b) = create_node(&app, "wsb").await;

    // ---- B 真实 TCP server ----
    let peer_endpoint = app.serve_on_random_port().await;

    // ---- B 签发配对码，A 建联（真实 HTTP verify）----
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/organization/links/pairing/issue",
            &IssuePairingCodeRequest {},
            &jwt_b,
        )
        .await;
    let data = common::assert_api_ok(status, &body);
    let pairing_code = data
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
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "create_link should succeed, got body: {}",
        body
    );

    // ---- A 侧 link（含 B 发给出站用的 access_token）----
    let ctx = RequestContext::from_storage("ws-test-assert", ai_orz::pkg::storage::get().clone());
    let link_a = ai_orz::service::dao::organization_link::dao()
        .find_by_pair(ctx.clone(), &org_a_id, &org_b_id)
        .await
        .expect("query A→B link failed")
        .expect("A→B link should exist after create_link");
    assert_eq!(link_a.access_token.len(), 64, "access_token 64-hex");

    // ---- A 拨 B 的 WS 端点（凭证 = B 发的 token）----
    let ws_url = ws::ws_url_from_base(&peer_endpoint);
    let state = ws::dial_peer(
        &org_a_id,
        &org_b_id,
        ws_url,
        link_a.access_token.clone(),
        None,
    )
    .await
    .expect("dial peer B should succeed");

    // 等待建连（supervisor 异步建连 + B 端 on_connected 注册）
    let mut connected = false;
    for _ in 0..100 {
        if state.is_connected().await && ws::registry().connected(&org_b_id) {
            connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(connected, "WS connection to B should be established");

    // ---- 命令往返：send_task 非法参数 → B consumer 回错误响应帧 ----
    // （响应帧到达 = pending Ok(payload)；业务错误语义由 payload.ok=false 携带，
    //   `call_peer` facade 的 parse_ws_send_response 负责把它转成 Err）
    let payload = json!({"id": "t1"}); // 缺 message 字段 → 反序列化失败
    let reply = ws::request_over_ws(
        &org_b_id,
        FEDERATION_CMD_SEND_TASK,
        uuid::Uuid::now_v7().to_string(),
        payload,
    )
    .await
    .expect("response frame should wake pending");
    assert_eq!(
        reply.get("ok").and_then(|v| v.as_bool()),
        Some(false),
        "business error flag should be false: {}",
        reply
    );
    assert!(
        reply
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("invalid send_task params"))
            .unwrap_or(false),
        "error message should mention invalid params: {}",
        reply
    );

    // ---- 幂等：pending 表无残留（响应后自动清理）----
    // （间接验证：第二次往返同样能成功唤醒，不受脏 pending 影响）
    let reply2 = ws::request_over_ws(
        &org_b_id,
        FEDERATION_CMD_SEND_TASK,
        uuid::Uuid::now_v7().to_string(),
        json!({"id": "t2"}),
    )
    .await
    .expect("second response frame should wake pending");
    assert_eq!(reply2.get("ok").and_then(|v| v.as_bool()), Some(false));

    ai_orz::pkg::ws::stop_client_shared(state).await;
}

/// e2e：无凭证拨号 → B 端握手 401 拒绝 → client 连不上（连接保护）
#[sqlx::test]
async fn test_federation_ws_rejects_bad_credential(pool: SqlitePool) {
    let _ = common::init_full_test_env(pool.clone()).await;
    let app = common::TestApp::new(pool).await;

    let (org_a_id, _jwt_a) = create_node(&app, "wsra").await;
    let (org_b_id, _jwt_b) = create_node(&app, "wsrb").await;
    let peer_endpoint = app.serve_on_random_port().await;

    // 错误 token 拨号（B 端没有任何 link，鉴权必然失败）
    let ws_url = ws::ws_url_from_base(&peer_endpoint);
    let state = ws::dial_peer(&org_a_id, &org_b_id, ws_url, "f".repeat(64), None)
        .await
        .expect("dial_peer itself succeeds (supervisor)");

    // supervisor 会重试，但 B 始终 401 → 永远不会注册为已连接
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !state.is_connected().await,
        "bad credential must never reach Connected"
    );
    assert!(
        !ws::registry().connected(&org_b_id),
        "rejected session must not be registered"
    );

    ai_orz::pkg::ws::stop_client_shared(state).await;
}
