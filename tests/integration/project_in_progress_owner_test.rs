//! Integration test for `ProjectManage::list_in_progress_with_owner`.
//!
//! Verifies that the domain method:
//! - Returns InProgress projects that have `owner_agent_id` set
//! - Filters out InProgress projects without `owner_agent_id`
//! - Filters out Completed projects (even if they have `owner_agent_id`)
//!
//! Test setup uses the HTTP API to create projects + agents (real end-to-end),
//! then calls the domain method directly to verify the system-level query
//! semantics required by the Agent Loop Engine.

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::service::domain::project::domain as project_domain;
use serde_json::json;
use sqlx::SqlitePool;

/// Create a project via HTTP API with the given `owner_agent_id` (optional).
///
/// Returns the new project's ID.
async fn create_project_with_owner(
    app: &TestApp,
    jwt: &str,
    name: &str,
    owner_agent_id: Option<&str>,
) -> String {
    let mut req = json!({
        "name": name,
        "description": "Test project for list_in_progress_with_owner",
    });
    if let Some(agent_id) = owner_agent_id {
        req["owner_agent_id"] = json!(agent_id);
    }
    let (status, body) = app.post_with_jwt("/api/v1/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in project create response")
        .to_string()
}

/// Transition a project's status via HTTP API (PUT /projects/{id}/status).
async fn transition_project_status(app: &TestApp, jwt: &str, project_id: &str, status: &str) {
    let req = json!({
        "id": project_id,
        "status": status,
    });
    let (status_code, body) = app
        .put_with_jwt(
            &format!("/api/v1/projects/{}/status", project_id),
            &req,
            jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "status transition to {} should succeed, body: {}",
        status,
        body
    );
}

/// `list_in_progress_with_owner` returns InProgress projects with `owner_agent_id`,
/// and filters out:
/// - InProgress projects without `owner_agent_id`
/// - Completed projects (even with `owner_agent_id`)
#[sqlx::test]
async fn test_list_in_progress_with_owner_filters(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create an Agent to use as owner_agent_id
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("ProjectOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // 1. InProgress + owner_agent_id → should be returned
    let p_in_progress_owner = create_project_with_owner(
        &app,
        &jwt,
        &format!("InProgressWithOwner-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    // Default status is Active (1); transition to InProgress (3)
    transition_project_status(&app, &jwt, &p_in_progress_owner, "InProgress").await;

    // 2. InProgress + NO owner_agent_id → should be filtered out
    let p_in_progress_no_owner = create_project_with_owner(
        &app,
        &jwt,
        &format!("InProgressNoOwner-{}", uuid::Uuid::now_v7()),
        None,
    )
    .await;
    transition_project_status(&app, &jwt, &p_in_progress_no_owner, "InProgress").await;

    // 3. Completed + owner_agent_id → should be filtered out
    let p_completed_owner = create_project_with_owner(
        &app,
        &jwt,
        &format!("CompletedWithOwner-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    // Active → InProgress → Completed (status machine requires intermediate step)
    transition_project_status(&app, &jwt, &p_completed_owner, "InProgress").await;
    transition_project_status(&app, &jwt, &p_completed_owner, "Completed").await;

    // Call the domain method directly (system-level query, no user filter)
    let projects = project_domain()
        .project_manage()
        .list_in_progress_with_owner(ctx)
        .await
        .expect("list_in_progress_with_owner should succeed");

    // Verify the in-progress + owner project is in the results
    let found_in_progress_owner = projects.iter().any(|p| p.po.id == p_in_progress_owner);
    assert!(
        found_in_progress_owner,
        "InProgress project with owner_agent_id should be returned"
    );

    // Verify the in-progress + no-owner project is filtered out
    let found_in_progress_no_owner = projects.iter().any(|p| p.po.id == p_in_progress_no_owner);
    assert!(
        !found_in_progress_no_owner,
        "InProgress project without owner_agent_id should be filtered out"
    );

    // Verify the completed + owner project is filtered out
    let found_completed_owner = projects.iter().any(|p| p.po.id == p_completed_owner);
    assert!(
        !found_completed_owner,
        "Completed project with owner_agent_id should be filtered out"
    );

    // All returned projects must satisfy both invariants:
    // - status is InProgress
    // - owner_agent_id is Some
    use ::common::enums::ProjectStatus;
    for p in &projects {
        assert_eq!(
            p.po.status,
            ProjectStatus::InProgress,
            "all returned projects should be InProgress, got {:?} for {}",
            p.po.status,
            p.po.id
        );
        assert!(
            p.po.owner_agent_id.is_some(),
            "all returned projects should have owner_agent_id set, got None for {}",
            p.po.id
        );
    }
}
