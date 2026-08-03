//! Integration tests for Agent management HTTP endpoints.
//!
//! Covers:
//! - Agent lifecycle status transitions (valid + invalid)
//! - External Agent creation (Cli / Remote)
//! - Agent search / query endpoints
//! - Tool pack install / uninstall / list
//! - Skill pack install / uninstall / list
//! - Get agent with stats query params
//! - Reception agent resolution
//! - Edge cases (not found, missing fields)

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Smoke test: create agent via factory, get it back, verify name.
#[sqlx::test]
async fn test_agent_smoke(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_name = format!("SmokeAgent-{}", uuid::Uuid::now_v7());
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &agent_name,
    )
    .await;

    // Verify the agent exists and name matches
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("name").and_then(|v| v.as_str()),
        Some(agent_name.as_str())
    );
    // New agent should be in Interviewing status (1)
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(1),
        "new agent should be Interviewing (1)"
    );
}

/// Full agent lifecycle: Interviewing → PendingOnboard → Onboarded →
/// PendingOffboard → Offboarded.
///
/// Verifies:
/// - Each transition returns HTTP 200 + code=0
/// - The `status` field in the response reflects the new status
/// - Onboarded transition auto-installs the "project_management" tool pack tag
#[sqlx::test]
async fn test_agent_lifecycle_valid_transitions(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("LifecycleAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Interviewing (1) → PendingOnboard (2)
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "PendingOnboard"}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(2),
        "should be PendingOnboard (2)"
    );

    // PendingOnboard (2) → Onboarded (3)
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "Onboarded"}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(3),
        "should be Onboarded (3)"
    );

    // Verify Onboarded auto-installed "project_management" tool pack tag
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs", agent_id),
            &jwt,
        )
        .await;
    let tp_data = crate::common::assert_api_ok(status, &body);
    let installed_tags = tp_data
        .get("installed_tags")
        .and_then(|v| v.as_array())
        .expect("installed_tags should be present");
    let has_pm = installed_tags
        .iter()
        .any(|t| t.as_str() == Some("project_management"));
    assert!(
        has_pm,
        "Onboarded agent should have project_management tool pack auto-installed"
    );

    // Onboarded (3) → PendingOffboard (5)
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "PendingOffboard"}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(5),
        "should be PendingOffboard (5)"
    );

    // PendingOffboard (5) → Offboarded (4)
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "Offboarded"}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(4),
        "should be Offboarded (4)"
    );
}

/// Invalid status transitions should be rejected.
///
/// Interviewing → Onboarded (skipping PendingOnboard) is illegal.
/// The API should return a non-zero error code.
#[sqlx::test]
async fn test_agent_lifecycle_invalid_transition_rejected(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("InvalidTransitionAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Interviewing (1) → Onboarded (3) — illegal, must skip PendingOnboard
    let (status, body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}/status", agent_id),
            &json!({"id": agent_id, "status": "Onboarded"}),
            &jwt,
        )
        .await;
    crate::common::assert_api_error(status, &body, axum::http::StatusCode::BAD_REQUEST);

    // Verify agent is still in Interviewing (1) — transition was rejected
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(1),
        "agent should still be Interviewing (1) after rejected transition"
    );
}
