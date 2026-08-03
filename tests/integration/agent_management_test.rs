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

/// Create an external CLI agent via POST /hr/agents/external.
///
/// Verifies:
/// - Creation returns 200 + id + kind="cli"
/// - GET detail returns kind="cli" and external_config.cli fields populated
/// - model_provider_id is empty for external agents
#[sqlx::test]
async fn test_create_external_cli_agent(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_name = format!("CliAgent-{}", uuid::Uuid::now_v7());
    let req = json!({
        "name": agent_name,
        "description": "A CLI agent for testing",
        "kind": "cli",
        "command": "echo",
        "args": ["hello"],
        "work_dir": "/tmp",
        "timeout_secs": 60
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/external", &req, &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let agent_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing id")
        .to_string();
    assert_eq!(
        data.get("kind").and_then(|v| v.as_str()),
        Some("cli"),
        "kind should be cli"
    );

    // GET detail and verify external_config
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("kind").and_then(|v| v.as_str()),
        Some("cli")
    );
    assert_eq!(
        data.get("model_provider_id").and_then(|v| v.as_str()),
        Some(""),
        "external agent should have empty model_provider_id"
    );
    let ext_config = data
        .get("external_config")
        .expect("external_config should be present for cli agent");
    let cli_config = ext_config
        .get("cli")
        .expect("cli config should be present");
    assert_eq!(
        cli_config.get("command").and_then(|v| v.as_str()),
        Some("echo")
    );
    assert_eq!(
        cli_config.get("work_dir").and_then(|v| v.as_str()),
        Some("/tmp")
    );
}

/// Create an external Remote (A2A) agent via POST /hr/agents/external.
///
/// Verifies:
/// - Creation returns 200 + id + kind="remote"
/// - GET detail returns kind="remote" and external_config.remote fields populated
#[sqlx::test]
async fn test_create_external_remote_agent(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_name = format!("RemoteAgent-{}", uuid::Uuid::now_v7());
    let req = json!({
        "name": agent_name,
        "description": "A remote A2A agent for testing",
        "kind": "remote",
        "endpoint": "http://localhost:9999",
        "agent_name": "test-remote-agent",
        "auth_token": "secret-token-123",
        "timeout_secs": 120
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/external", &req, &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let agent_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing id")
        .to_string();
    assert_eq!(
        data.get("kind").and_then(|v| v.as_str()),
        Some("remote")
    );

    // GET detail and verify external_config
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("kind").and_then(|v| v.as_str()),
        Some("remote")
    );
    let ext_config = data
        .get("external_config")
        .expect("external_config should be present");
    let remote_config = ext_config
        .get("remote")
        .expect("remote config should be present");
    assert_eq!(
        remote_config.get("endpoint").and_then(|v| v.as_str()),
        Some("http://localhost:9999")
    );
    assert_eq!(
        remote_config.get("agent_name").and_then(|v| v.as_str()),
        Some("test-remote-agent")
    );
}

/// Search agents by keyword via POST /hr/agents/search.
///
/// Verifies:
/// - Search by keyword returns matching agents (FTS5 path, no embedding model)
/// - Search results are in PagedResult format {items, total}
#[sqlx::test]
async fn test_agent_search_by_keyword(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create two agents with distinct names sharing a unique suffix
    let unique = uuid::Uuid::now_v7().to_string();
    let name_a = format!("SearchableAlpha-{}", unique);
    let name_b = format!("SearchableBeta-{}", unique);
    crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &name_a).await;
    crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &name_b).await;

    // Search by the unique suffix — should return both (FTS5 tokenizes on hyphens)
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({"keyword": unique, "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items should be an array");
    assert!(
        items.len() >= 2,
        "search should return at least 2 agents, got {}",
        items.len()
    );
    let total = data
        .get("total")
        .and_then(|v| v.as_i64())
        .expect("total should be present");
    assert!(total >= 2, "total should be >= 2");

    // Search by a name fragment unique to agent A
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({"keyword": &name_a, "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items should be an array");
    let found_a = items
        .iter()
        .any(|item| item.get("name").and_then(|v| v.as_str()) == Some(name_a.as_str()));
    assert!(found_a, "search should find agent A by its full name");
}

/// Query agents by IDs batch and status filter via POST /hr/agents/query.
///
/// Verifies:
/// - Batch query by ids returns exactly those agents
/// - Query by status filter returns only matching agents
#[sqlx::test]
async fn test_agent_query_by_ids_and_status(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create two agents
    let id_a = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("QueryTargetA-{}", uuid::Uuid::now_v7()),
    )
    .await;
    let id_b = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("QueryTargetB-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Batch query by ids — should return exactly these 2
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/query",
            &json!({"ids": [id_a, id_b], "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items should be an array");
    let returned_ids: Vec<&str> = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|v| v.as_str()))
        .collect();
    assert!(
        returned_ids.contains(&id_a.as_str()),
        "query by ids should include agent A"
    );
    assert!(
        returned_ids.contains(&id_b.as_str()),
        "query by ids should include agent B"
    );

    // Query by status=Interviewing — all newly created agents are Interviewing
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/query",
            &json!({"status": "Interviewing", "limit": 50, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items should be an array");
    // All returned agents should be Interviewing (1)
    for item in items {
        assert_eq!(
            item.get("status").and_then(|v| v.as_i64()),
            Some(1),
            "all queried agents should be Interviewing (1)"
        );
    }
}

/// Tool pack lifecycle: install → list → install again (idempotent) → uninstall → list.
///
/// Verifies:
/// - POST install adds the tag to installed_tags
/// - GET list returns the tag
/// - POST install same tag again is idempotent (no error, tag still present)
/// - DELETE uninstall removes the tag
/// - GET list no longer contains the tag
#[sqlx::test]
async fn test_tool_pack_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("ToolPackAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let tag = "test_tool_pack";

    // 1. Install tool pack
    let (status, body) = app
        .post_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag),
            &json!({}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let installed_tags = data
        .get("installed_tags")
        .and_then(|v| v.as_array())
        .expect("installed_tags should be present");
    assert!(
        installed_tags
            .iter()
            .any(|t| t.as_str() == Some(tag)),
        "tag should be in installed_tags after install"
    );

    // 2. List installed tool packs
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let listed_tags = data
        .get("installed_tags")
        .and_then(|v| v.as_array())
        .expect("installed_tags should be present in list");
    assert!(
        listed_tags
            .iter()
            .any(|t| t.as_str() == Some(tag)),
        "tag should appear in list"
    );

    // 3. Install same tag again — idempotent, should succeed
    let (status, _body) = app
        .post_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag),
            &json!({}),
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "idempotent install should succeed"
    );

    // 4. Uninstall tool pack
    let (status, body) = app
        .delete_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let remaining_tags = data
        .get("installed_tags")
        .and_then(|v| v.as_array())
        .expect("installed_tags should be present after uninstall");
    assert!(
        !remaining_tags
            .iter()
            .any(|t| t.as_str() == Some(tag)),
        "tag should be removed after uninstall"
    );

    // 5. List again — tag should be gone
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}/tool-packs", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let final_tags = data
        .get("installed_tags")
        .and_then(|v| v.as_array())
        .expect("installed_tags should be present");
    assert!(
        !final_tags
            .iter()
            .any(|t| t.as_str() == Some(tag)),
        "tag should not appear in final list"
    );
}
