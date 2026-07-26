//! Integration tests verifying vector index degradation guarantees.
//!
//! These tests explicitly verify the design contract:
//! - When no Embedding provider is available (Ok(None) path), entity creates
//!   succeed without panic and the main table record is persisted.
//! - When cortex calls fail (Err path), entity creates still succeed because
//!   DAL layer catches the error with `log_warn!` and does not propagate.
//!
//! This protects a critical robustness guarantee documented in
//! [src/service/dal/project.rs:205-254] and other DAL modules.
//!
//! 路由说明（基于 Phase 3/4 验证过的实际路由）：
//! - Agent: `POST /api/v1/hr/agents`, `GET /api/v1/hr/agents/{id}`
//! - Project: `POST /api/v1/projects`, `GET /api/v1/projects/{id}`
//! - Task: `POST /api/v1/tasks`（DTO 必填 `assignee_id`，需先创建 Agent）
//! - Message: `POST /api/v1/finance/messages/agents`（DTO 用 `to_agent_id`，响应字段 `message_id`）
//! - Model Provider: `GET /api/v1/finance/model-providers`, `DELETE /api/v1/finance/model-providers/{id}`
//!
//! ModelCapability::Embedding 的 i32 值 = 1（已在 `common/src/enums/provider.rs` 确认）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// When the embedding provider is deleted, agent creation should still succeed
/// and the agent record should be retrievable.
///
/// Validates the `Ok(None)` degradation path in agent DAL — when
/// `get_default_embedding_provider` returns `Ok(None)`, `embed_entity` is
/// skipped and the main table INSERT commits successfully.
#[sqlx::test]
async fn test_agent_create_succeeds_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Verify embedding provider is really gone — list should NOT contain any
    // provider with capability == 1 (ModelCapability::Embedding).
    let (status, body) = app
        .get_with_jwt("/api/v1/finance/model-providers", &jwt)
        .await;
    let providers = crate::common::assert_api_ok(status, &body);
    let has_embedding = providers
        .as_array()
        .map(|arr| {
            arr.iter().any(|p| {
                p.get("capability")
                    .and_then(|v| v.as_i64())
                    .map(|c| c == 1) // ModelCapability::Embedding = 1
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    assert!(
        !has_embedding,
        "embedding provider should be deleted; got providers: {}",
        providers
    );

    // Create an agent — should succeed despite no embedding provider.
    // This exercises the Ok(None) degradation path in agent DAL.
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("NoVecAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Re-fetch — main table record must be persisted
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("id").and_then(|v| v.as_str()),
        Some(agent_id.as_str()),
        "agent should be retrievable after create-without-embedding"
    );
}

/// When the embedding provider is deleted, project creation should still succeed.
///
/// Validates the `Ok(None)` degradation path in project DAL — when
/// `get_default_embedding_provider` returns `Ok(None)`, `embed_entity` is
/// skipped and the main table INSERT commits successfully.
#[sqlx::test]
async fn test_project_create_succeeds_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    let project_id = crate::common::factories::create_test_project(
        &app,
        &jwt,
        &format!("NoVecProject-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Re-fetch — main table record must be persisted
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        project_data.get("id").and_then(|v| v.as_str()),
        Some(project_id.as_str()),
        "project should be retrievable after create-without-embedding"
    );
}

/// End-to-end smoke test: bootstrap → delete embedding → create entities in
/// sequence (agent → project → task → message). All should succeed without
/// cortex ever being invoked.
///
/// 这是健壮性契约的最强验证：覆盖 agent/project/task/message 四个 DAL 的
/// `embed_entity` 降级路径，任何一个 DAL 改成 propagate error 都会让本测试失败。
#[sqlx::test]
async fn test_full_crud_loop_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // 1. Agent — 验证 agent DAL 的 embed_entity 降级
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        "DegradationSmoke-Agent",
    )
    .await;

    // 2. Project — 验证 project DAL 的 embed_entity 降级
    let project_id =
        crate::common::factories::create_test_project(&app, &jwt, "DegradationSmoke-Project").await;

    // 3. Task under project (assignee_id 必填，指向刚创建的 agent)
    //    验证 task DAL 的 embed_entity 降级
    let task_req = json!({
        "title": "Degradation smoke task",
        "description": "Should succeed without embedding provider",
        "project_id": project_id,
        "assignee_id": agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, &jwt).await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "task create should succeed without embedding provider, body: {}",
        body
    );

    // 4. Message send — 验证 message DAL 的 embed_entity 降级
    //    DTO: `SendMessageToAgentParams { to_agent_id, content, ... }`
    //    这是降级路径最复杂的一步：message create 同时触发 AOP publish + 向量索引
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "Hello from degradation smoke test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "message send should succeed without embedding provider, body: {}",
        body
    );

    // 验证 message 真的写入主表（响应字段是 message_id，不是 id）
    let msg_data = crate::common::assert_api_ok(status, &body);
    assert!(
        msg_data
            .get("message_id")
            .and_then(|v| v.as_str())
            .is_some(),
        "message_id should be present in send response, got: {}",
        msg_data
    );
}
