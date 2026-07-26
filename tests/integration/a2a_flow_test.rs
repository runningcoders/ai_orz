//! Integration tests for the A2A (Agent-to-Agent) protocol flow.
//!
//! Covers:
//! - `GET /.well-known/agent.json` returns the agent card (public, no JWT)
//! - `POST /a2a` JSON-RPC `tasks/send` → `tasks/get` round trip (JWT protected)
//!
//! 路由说明（基于 src/router.rs 实际路由）：
//! - Agent Card: `GET /.well-known/agent.json`（公开，仅需 RequestContext）
//! - JSON-RPC: `POST /a2a`（JWT 保护，需 a2a_server.enabled = true）
//!
//! A2A tasks/send 流程（src/handlers/a2a/send_task.rs）：
//! 1. resolve_agent 查找 Onboarded 状态的前台 Agent
//! 2. 创建 project（对应 A2A task）
//! 3. 创建 message → 入队 event_queue（consumer 异步消费，测试不依赖 consumer）
//! 4. 立即返回 working 状态的 A2aTask

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// `GET /.well-known/agent.json` returns a valid agent card.
///
/// This endpoint is public (no JWT required) and always available regardless
/// of `a2a_server.enabled` config — it only returns a static card describing
/// the organization's capabilities.
#[sqlx::test]
async fn test_agent_card_discovery(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (status, body) = app.get("/.well-known/agent.json").await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "agent card endpoint should return 200"
    );
    // Agent card must expose capabilities per A2A spec
    let capabilities = body
        .get("capabilities")
        .unwrap_or_else(|| panic!("agent card should expose capabilities, got: {}", body));
    assert!(
        capabilities.get("streaming").is_some() || capabilities.get("push_notifications").is_some(),
        "capabilities should have streaming or push_notifications, got: {}",
        capabilities
    );
    // Verify other required agent card fields
    assert!(
        body.get("name").is_some(),
        "agent card should expose name, got: {}",
        body
    );
    assert!(
        body.get("version").is_some(),
        "agent card should expose version, got: {}",
        body
    );
    assert!(
        body.get("skills").is_some(),
        "agent card should expose skills, got: {}",
        body
    );
}

/// A2A JSON-RPC `tasks/send` then `tasks/get` round trip.
///
/// This test verifies the full protocol plumbing:
/// 1. Bootstrap system + create an Onboarded agent (required by resolve_agent)
/// 2. `tasks/send` creates a project + message, returns a working-state task
/// 3. `tasks/get` retrieves the same task by id
///
/// The test does not require the AOP consumer to be running — `tasks/send`
/// returns immediately with `working` status, and `tasks/get` returns the
/// project + any messages (including the user's initial message).
#[sqlx::test]
async fn test_a2a_tasks_send_then_get(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // Bootstrap + login + disable embedding (degradation path, no cortex calls)
    let (bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Create an agent and transition to Onboarded (required by resolve_agent)
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("A2A-Agent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Transition: Interviewing → PendingOnboard → Onboarded
    // (AgentStatus serializes as variant name string, e.g. "Onboarded")
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "PendingOnboard"}),
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "transition to PendingOnboard should succeed, body: {}",
        body
    );

    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "Onboarded"}),
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "transition to Onboarded should succeed, body: {}",
        body
    );

    // tasks/send via JSON-RPC
    // SendTaskParams requires `id` (client-generated), `message` (A2aMessage)
    // A2aMessagePart uses `{"type": "text", "text": "..."}` (tag = "type", snake_case)
    let send_rpc = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "tasks/send",
        "params": {
            "id": format!("client-task-{}", uuid::Uuid::now_v7()),
            "message": {
                "role": "user",
                "parts": [{"type": "text", "text": "Hello A2A"}]
            }
        }
    });
    let (status, body) = app.post_with_jwt("/a2a", &send_rpc, &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tasks/send should return 200, body: {}",
        body
    );
    // Verify JSON-RPC success response (result present, error absent)
    assert!(
        body.get("result").is_some(),
        "tasks/send should return result, got: {}",
        body
    );
    assert!(
        body.get("error").is_none(),
        "tasks/send should not return error, got: {}",
        body
    );
    // Extract task id from result.id (= project_id in ai_orz)
    let task_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks/send result should contain task id")
        .to_string();

    // tasks/get via JSON-RPC
    let get_rpc = json!({
        "jsonrpc": "2.0",
        "id": "2",
        "method": "tasks/get",
        "params": {
            "id": task_id
        }
    });
    let (status, body) = app.post_with_jwt("/a2a", &get_rpc, &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tasks/get should return 200, body: {}",
        body
    );
    assert!(
        body.get("result").is_some(),
        "tasks/get should return result, got: {}",
        body
    );
    assert!(
        body.get("error").is_none(),
        "tasks/get should not return error, got: {}",
        body
    );
    // Verify the same task id is returned
    let returned_id = body
        .get("result")
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str());
    assert_eq!(
        returned_id,
        Some(task_id.as_str()),
        "tasks/get should return the same task id"
    );
}
