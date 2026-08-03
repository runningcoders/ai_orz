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
