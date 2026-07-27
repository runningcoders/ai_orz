//! Integration tests for the message delivery pipeline.
//!
//! Covers:
//! - `POST /finance/messages/agents` sends a message to an agent; record persists
//!   and is retrievable via `GET /finance/messages?to_id=...`
//! - `GET /finance/messages/sse` SSE endpoint connection smoke test
//!
//! 路由说明（基于 `src/router.rs::finance_routes` 实际注册）：
//! - 发送消息端点是 `/api/v1/finance/messages/agents`（不是计划文档中的 `/messages/send`）
//! - 发送响应 DTO 是 `SendMessageToAgentResponse { message_id }`（不是 `id`）
//! - 列表响应 DTO 是 `ListMessagesResponse { messages, total }`（不是 PagedResult `{items, total}`）
//!   且列表项字段名是 `message_id`（不是 `id`）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Send a message to an agent — verifies the message record is persisted
/// with the correct recipient/content and is retrievable via the list endpoint.
///
/// This is the entry point of the delivery pipeline (AOP enqueue happens
/// inside `send_to_agent` but is not asserted here — covered by unit tests).
#[sqlx::test]
async fn test_send_message_persists_record(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create an agent to receive the message
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("MsgReceiver-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Send a message to the agent.
    // DTO: `SendMessageToAgentParams { to_agent_id, content, project_id?, ... }`
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "Hello from integration test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id in send response")
        .to_string();

    // List messages filtered by to_id — should contain our new message.
    // DTO: `ListMessagesResponse { messages: Vec<MessageListItem>, total }`,
    // each item's id field is `message_id`.
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/finance/messages?to_id={}", agent_id),
            &jwt,
        )
        .await;
    let list_data = crate::common::assert_api_ok(status, &body);
    let found = list_data
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().any(|item| {
                item.get("message_id").and_then(|v| v.as_str()) == Some(message_id.as_str())
            })
        })
        .unwrap_or(false);
    assert!(
        found,
        "sent message should appear in list filtered by to_id"
    );

    // Sanity: the persisted content should match what we sent
    let content_match = list_data
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("content"))
        .and_then(|v| v.as_str())
        == Some("Hello from integration test");
    assert!(
        content_match,
        "persisted message content should match the sent payload, got: {}",
        list_data
    );
}

/// SSE subscription endpoint should return 200 OK on connection.
///
/// This is a connection-level smoke test — we do NOT read the body because
/// SSE streams never end (the handler sets a 15s keep-alive ping), so
/// `to_bytes(usize::MAX)` would hang forever. We only assert the status code
/// returned by the initial response, which proves:
/// - the route is wired correctly
/// - JWT auth passes
/// - `subscribe_sse` domain call succeeds (the handler does `.unwrap()` on it)
#[sqlx::test]
async fn test_sse_endpoint_returns_event_stream(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let status = app
        .get_with_jwt_status_only("/api/v1/finance/messages/sse", &jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "SSE endpoint should return 200 on connection"
    );
}
