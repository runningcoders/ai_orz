//! Integration tests for core business CRUD loops.
//!
//! Covers:
//! - Agent create → list → get → update → delete → 404
//! - Project create → update status transitions → verify → archive
//! - Task create → update progress → transition to InProgress → mark Completed

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Full Agent CRUD loop:
/// 1. Create agent (returns id)
/// 2. List agents (should contain the new id)
/// 3. Get agent by id (should match)
/// 4. Update agent name
/// 5. Delete agent
/// 6. Get by id should now fail (404 or non-zero code)
#[sqlx::test]
async fn test_agent_crud_loop(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // 删除 embedding provider 走向量降级路径，避免触发 FastEmbed 模型下载
    let (bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // 直接使用 bootstrap 返回的 chat_provider_id（无需再发 HTTP 请求查询）
    let provider_id = &bs.chat_provider_id;

    // 1. Create agent
    let agent_name = format!("TestAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, provider_id, &agent_name).await;

    // 2. List agents — should contain our new id (response is PagedResult: {items: [...], total: N})
    let (status, body) = app.get_with_jwt("/api/v1/hr/agents", &jwt).await;
    let list_data = crate::common::assert_api_ok(status, &body);
    let found_in_list = list_data
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .any(|item| item.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()))
        })
        .unwrap_or(false);
    assert!(found_in_list, "created agent should appear in list");

    // 3. Get agent by id
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("name").and_then(|v| v.as_str()),
        Some(agent_name.as_str()),
        "fetched agent name should match"
    );

    // 4. Update agent name
    let new_name = format!("UpdatedAgent-{}", uuid::Uuid::now_v7());
    let update_req = json!({
        "id": agent_id,
        "name": new_name,
        "description": "Updated agent",
        "capabilities": ["chat"],
        "soul": "Updated soul",
        "model_provider_id": provider_id,
    });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}", agent_id),
            &update_req,
            &jwt,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "update should succeed");

    // Re-fetch and verify name changed
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("name").and_then(|v| v.as_str()),
        Some(new_name.as_str()),
        "name should be updated"
    );

    // 5. Delete agent
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "delete should succeed");

    // 6. Re-fetch should fail (404 or non-zero code)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    assert!(
        status == axum::http::StatusCode::NOT_FOUND
            || body.get("code").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        "deleted agent should not be retrievable"
    );

    // bs 保留以验证完整 bootstrap 返回的字段
    let _ = bs;
}

/// Project create → update status transitions → archive.
///
/// 注意：项目没有 DELETE 路由（软删除通过 Archived 状态实现），
/// 所以用 Active → InProgress → Archived 完成完整生命周期验证。
#[sqlx::test]
async fn test_project_status_transitions(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // Create project
    let project_name = format!("TestProject-{}", uuid::Uuid::now_v7());
    let project_id = crate::common::factories::create_test_project(&app, &jwt, &project_name).await;

    // Get project — verify initial state (Active = 1)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        project_data.get("name").and_then(|v| v.as_str()),
        Some(project_name.as_str())
    );
    let initial_status = project_data
        .get("status")
        .and_then(|v| v.as_i64())
        .expect("project status field should be present");
    assert_eq!(
        initial_status, 1,
        "newly created project should be Active (1)"
    );

    // Update project status: Active → InProgress (PUT, not POST)
    let status_req = json!({
        "id": project_id,
        "status": "InProgress"
    });
    let (status_code, body) = app
        .put_with_jwt(
            &format!("/api/v1/projects/{}/status", project_id),
            &status_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "status update to InProgress should succeed, body: {}",
        body
    );

    // Re-fetch and verify status changed to InProgress (3)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    let updated_status = project_data
        .get("status")
        .and_then(|v| v.as_i64())
        .expect("project status should be present after update");
    assert_eq!(
        updated_status, 3,
        "project status should be InProgress (3) after transition"
    );

    // Archive the project: InProgress → Archived (替代 DELETE，因为项目无删除路由)
    let archive_req = json!({
        "id": project_id,
        "status": "Archived"
    });
    let (status_code, body) = app
        .put_with_jwt(
            &format!("/api/v1/projects/{}/status", project_id),
            &archive_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "archive (InProgress → Archived) should succeed, body: {}",
        body
    );

    // Verify final status is Archived (5)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/projects/{}", project_id), &jwt)
        .await;
    let project_data = crate::common::assert_api_ok(status, &body);
    let final_status = project_data
        .get("status")
        .and_then(|v| v.as_i64())
        .expect("project status should be present after archive");
    assert_eq!(
        final_status, 5,
        "project status should be Archived (5) after archive transition"
    );
}

/// Task create → update progress → transition to InProgress → mark Completed.
///
/// 注意：
/// - CreateTaskRequest 的 `assignee_id` 是必填字段，需要先创建 Agent 作为分配对象
/// - 路由中无 `/tasks/{id}/mark-done` 端点（handler 存在但未注册），
///   改用 `PUT /tasks/{id}/status` + status="Completed" 完成状态流转
/// - Task 状态流转规则：Pending(2) → InProgress(3) → Completed(4)
#[sqlx::test]
async fn test_task_progress_and_completion(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;

    // 1. 创建一个 Agent 作为 task 的 assignee（assignee_id 是必填字段）
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskAssignee-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // 2. 创建一个 Project 承载 task
    let project_id = crate::common::factories::create_test_project(
        &app,
        &jwt,
        &format!("TaskHost-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // 3. 在 project 下创建 task，assignee_id 指向刚创建的 agent
    let task_req = json!({
        "title": "Test task",
        "description": "Test task for integration",
        "project_id": project_id,
        "assignee_id": agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, &jwt).await;
    let task_data = crate::common::assert_api_ok(status, &body);
    let task_id = task_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id in create response")
        .to_string();

    // 验证初始状态为 Pending (2)
    let initial_status = task_data
        .get("status")
        .and_then(|v| v.as_i64())
        .expect("task status should be present");
    assert_eq!(
        initial_status, 2,
        "newly created task should be Pending (2)"
    );

    // 4. 更新 task 进度为 50%（PUT /tasks/{id}/progress，不是 POST）
    let progress_req = json!({
        "id": task_id,
        "progress": 50
    });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/tasks/{}/progress", task_id),
            &progress_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "progress update should succeed"
    );

    // 5. 状态流转：Pending → InProgress（PUT /tasks/{id}/status）
    let in_progress_req = json!({
        "id": task_id,
        "status": "InProgress"
    });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/tasks/{}/status", task_id),
            &in_progress_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "transition to InProgress should succeed"
    );

    // 6. 标记完成：InProgress → Completed（PUT /tasks/{id}/status，因为 mark-done 路由未注册）
    let completed_req = json!({
        "id": task_id,
        "status": "Completed"
    });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/tasks/{}/status", task_id),
            &completed_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "transition to Completed should succeed"
    );

    // 7. 重新获取 task，验证最终状态为 Completed (4)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/tasks/{}", task_id), &jwt)
        .await;
    let task_data = crate::common::assert_api_ok(status, &body);
    let final_status = task_data
        .get("status")
        .and_then(|v| v.as_i64())
        .expect("task status field should be present after completion");
    assert_eq!(
        final_status, 4,
        "task status should be Completed (4) after mark-done transition"
    );
}
