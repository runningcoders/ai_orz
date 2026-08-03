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

// ===== 真实向量搜索测试辅助（Part B）=====

/// Load .env file and read an env var. Returns None if unset or empty.
fn env_or_none(key: &str) -> Option<String> {
    let _ = dotenvy::dotenv();
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

/// Parse provider type string to serde variant name.
fn parse_provider_type(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "openai" | "0" => "OpenAI",
        "deepseek" | "1" => "DeepSeek",
        "qwen" | "2" => "Qwen",
        "doubao" | "3" => "Doubao",
        "ollama" | "4" => "Ollama",
        "custom" | "5" => "Custom",
        _ => "OpenAI",
    }
}

/// Real model test config parsed from environment variables.
struct RealModelConfig {
    embedding_api_key: String,
    embedding_model_name: String,
    embedding_provider_type: &'static str,
    embedding_base_url: Option<String>,
}

impl RealModelConfig {
    /// Load embedding config from env. Returns None if API key is missing.
    fn from_env() -> Option<Self> {
        let embedding_api_key = env_or_none("TEST_EMBEDDING_API_KEY")?;
        let embedding_model_name =
            env_or_none("TEST_EMBEDDING_MODEL_NAME").unwrap_or_else(|| "text-embedding-3-small".into());
        let embedding_provider_type = env_or_none("TEST_EMBEDDING_PROVIDER_TYPE")
            .as_deref()
            .map(parse_provider_type)
            .unwrap_or("OpenAI");
        let embedding_base_url = env_or_none("TEST_EMBEDDING_BASE_URL");
        Some(Self {
            embedding_api_key,
            embedding_model_name,
            embedding_provider_type,
            embedding_base_url,
        })
    }
}

/// Create a real Embedding ModelProvider via HTTP API. Returns the provider ID.
async fn create_embedding_provider(app: &TestApp, jwt: &str, cfg: &RealModelConfig) -> String {
    let req = json!({
        "name": format!("TestEmbedding-{}", uuid::Uuid::now_v7()),
        "provider_type": cfg.embedding_provider_type,
        "capability": "Embedding",
        "model_name": cfg.embedding_model_name,
        "api_key": cfg.embedding_api_key,
        "base_url": cfg.embedding_base_url,
        "description": "Real embedding provider for vector search tests",
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/model-providers", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing provider id")
        .to_string()
}

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

/// Skill pack lifecycle: install → list → uninstall → list.
///
/// Note: installing a skill pack tag with no matching Published skills still
/// records the tag in installed_skill_packs (installed_count=0).
/// This test uses a unique tag that no Published skill carries.
#[sqlx::test]
async fn test_skill_pack_lifecycle(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("SkillPackAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let tag = format!("test_skill_pack_{}", uuid::Uuid::now_v7());

    // 1. Install skill pack (no matching skills → installed_count=0, but tag recorded)
    let (status, body) = app
        .post_with_jwt(
            &format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag),
            &json!({}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("installed_count").and_then(|v| v.as_i64()),
        Some(0),
        "installed_count should be 0 when no matching skills exist"
    );

    // 2. List installed skill packs — tag should be present
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}/skill-packs", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let skill_packs = data
        .get("skill_packs")
        .and_then(|v| v.as_array())
        .expect("skill_packs should be present");
    assert!(
        skill_packs
            .iter()
            .any(|t| t.as_str() == Some(tag.as_str())),
        "tag should appear in skill_packs list after install"
    );

    // 3. Uninstall skill pack
    let (status, _body) = app
        .delete_with_jwt(
            &format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag),
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "uninstall skill pack should succeed"
    );

    // 4. List again — tag should be gone
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}/skill-packs", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let final_packs = data
        .get("skill_packs")
        .and_then(|v| v.as_array())
        .expect("skill_packs should be present");
    assert!(
        !final_packs
            .iter()
            .any(|t| t.as_str() == Some(tag.as_str())),
        "tag should not appear in skill_packs after uninstall"
    );
}

/// Get agent with with_stats=true query param.
///
/// Verifies:
/// - GET without with_stats: stats field is absent (skip_serializing_if = Option::is_none)
/// - GET with with_stats=true: stats field is present
#[sqlx::test]
async fn test_get_agent_with_stats(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("StatsAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Without with_stats — stats field should be absent (serde skip_serializing_if None)
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert!(
        data.get("stats").is_none(),
        "stats should be absent without with_stats=true"
    );

    // With with_stats=true — stats field should be present
    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/hr/agents/{}?with_stats=true", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert!(
        data.get("stats").is_some(),
        "stats should be present with with_stats=true"
    );
}

/// Reception agent resolution: GET /hr/agents/reception.
///
/// Verifies:
/// - With no onboarded agent → 404 error
/// - After onboarding an agent → returns the onboarded agent's id + name
#[sqlx::test]
async fn test_reception_agent_resolution(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. No onboarded agent yet → should get 404
    let (status, body) = app
        .get_with_jwt("/api/v1/hr/agents/reception", &jwt)
        .await;
    crate::common::assert_api_error(status, &body, axum::http::StatusCode::NOT_FOUND);

    // 2. Create an agent and onboard it
    let agent_name = format!("ReceptionAgent-{}", uuid::Uuid::now_v7());
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &agent_name,
    )
    .await;

    // Interviewing → PendingOnboard → Onboarded
    app.put_with_jwt(
        &format!("/api/v1/hr/agents/{}/status", agent_id),
        &json!({"id": agent_id, "status": "PendingOnboard"}),
        &jwt,
    )
    .await;
    app.put_with_jwt(
        &format!("/api/v1/hr/agents/{}/status", agent_id),
        &json!({"id": agent_id, "status": "Onboarded"}),
        &jwt,
    )
    .await;

    // 3. Now reception should resolve to an onboarded agent
    let (status, body) = app
        .get_with_jwt("/api/v1/hr/agents/reception", &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let resolved_id = data
        .get("agent_id")
        .and_then(|v| v.as_str())
        .expect("agent_id should be present in reception response");
    assert!(
        !resolved_id.is_empty(),
        "reception agent_id should not be empty"
    );
}

/// Edge cases:
/// - GET nonexistent agent → 404 or non-zero code
/// - DELETE nonexistent agent → 404 or non-zero code
/// - Create Local agent without model_provider_id → error (Local kind requires it)
#[sqlx::test]
async fn test_agent_edge_cases(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. GET nonexistent agent
    let fake_id = format!("nonexistent-{}", uuid::Uuid::now_v7());
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", fake_id), &jwt)
        .await;
    assert!(
        status == axum::http::StatusCode::NOT_FOUND
            || body.get("code").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        "getting nonexistent agent should fail: status={}, body={}",
        status,
        body
    );

    // 2. DELETE nonexistent agent
    let (status, body) = app
        .delete_with_jwt(&format!("/api/v1/hr/agents/{}", fake_id), &jwt)
        .await;
    assert!(
        status == axum::http::StatusCode::NOT_FOUND
            || body.get("code").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        "deleting nonexistent agent should fail: status={}, body={}",
        status,
        body
    );

    // 3. Create Local agent without model_provider_id → should error
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents",
            &json!({
                "name": "NoProviderAgent",
                "model_provider_id": ""
            }),
            &jwt,
        )
        .await;
    let _ = status;
    assert!(
        body.get("code").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        "creating Local agent with empty model_provider_id should fail: body={}",
        body
    );
}

/// Real vector search: create embedding provider → create agents with
/// semantically distinct descriptions → search by a keyword that does NOT
/// appear in the text but is semantically related.
///
/// This verifies the vector index is built and semantic recall works.
/// Uses Doubao embedding model via real API.
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_vector_semantic_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping vector search test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. Create real Embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. Create an agent with a description that does NOT contain the search keyword,
    //    but is semantically related.
    //    Description mentions "深度学习模型训练与梯度下降" — search keyword "神经网络"
    //    is semantically related but not a literal substring.
    let agent_name = format!("VectorSearchAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "这是一个专门负责深度学习模型训练与梯度下降优化的智能助手",
        "model_provider_id": bs.chat_provider_id,
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents", &agent_req, &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    let agent_id = agent_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing agent id")
        .to_string();

    // Wait for async vector indexing to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. Search by a semantically related keyword that is NOT in the text
    //    "神经网络" (neural network) is semantically close to "深度学习" (deep learning)
    //    but does not appear literally in the description.
    let search_req = json!({
        "keyword": "神经网络",
        "limit": 20,
        "offset": 0
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/search", &search_req, &jwt)
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items in search response");

    // The agent should be found via semantic (vector) similarity
    let found = items
        .iter()
        .any(|item| item.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(
        found,
        "agent should be found via semantic vector search for '神经网络' \
         (description mentions '深度学习'); items: {:?}",
        items
            .iter()
            .map(|i| i.get("name").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Vector semantic search test passed: agent found via semantic similarity");

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// Vector index auto-maintenance: update agent description → verify new
/// vector reflects updated text; delete agent → verify it disappears from
/// search results.
///
/// Uses real Embedding model to verify the full lifecycle of vector index
/// maintenance (create → update → delete).
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_vector_index_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping vector maintenance test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. Create embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. Create an agent with an initial description
    let agent_name = format!("VectorMaintAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "负责前端页面开发和用户界面设计的助手",
        "model_provider_id": bs.chat_provider_id,
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents", &agent_req, &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    let agent_id = agent_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing agent id")
        .to_string();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. Verify initial search finds agent by "前端" keyword
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({"keyword": "前端", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_before = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(found_before, "agent should be found before update");

    // 4. Update agent description to a completely different domain
    let update_req = json!({
        "id": agent_id,
        "name": agent_name,
        "description": "负责数据库管理和SQL查询优化的专家",
        "model_provider_id": bs.chat_provider_id,
    });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/hr/agents/{}", agent_id),
            &update_req,
            &jwt,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "update should succeed");

    // Wait for vector re-indexing
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 5. Search by "数据库" — should still find the agent (updated vector)
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({"keyword": "数据库", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after_update = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(
        found_after_update,
        "agent should be found via new description after update"
    );

    // 6. Delete the agent
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "delete should succeed");

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 7. Search again — agent should NOT appear
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({"keyword": "数据库", "limit": 50, "offset": 0}),
            &jwt,
        )
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let still_found = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(
        !still_found,
        "deleted agent should not appear in search results"
    );

    eprintln!("Vector index maintenance test passed: create → update → delete lifecycle verified");

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// Hybrid search ranking: create two agents — one matching by keyword (FTS5),
/// one matching by semantic similarity (vector). Search with a keyword that
/// matches one literally and the other semantically.
///
/// Verifies:
/// - Both agents appear in search results (hybrid: FTS5 + vector)
/// - The keyword-match agent ranks higher (FTS5 score > vector score)
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_hybrid_search_ranking(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping hybrid search test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. Create embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. Create two agents:
    //    Agent A: description contains the search keyword literally (FTS5 match)
    //    Agent B: description is semantically related but does NOT contain the keyword
    let unique = uuid::Uuid::now_v7().to_string();
    let name_a = format!("FtsMatchAgent-{}", unique);
    let name_b = format!("VectorMatchAgent-{}", unique);

    // Agent A: "自然语言处理" (contains the search keyword)
    let req_a = json!({
        "name": name_a,
        "description": "专注于自然语言处理和文本分析的助手",
        "model_provider_id": bs.chat_provider_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/agents", &req_a, &jwt).await;
    let agent_a_id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing id")
        .to_string();

    // Agent B: "语义理解与文本挖掘" (semantically related to NLP but no "自然语言处理")
    let req_b = json!({
        "name": name_b,
        "description": "负责语义理解和文本挖掘的智能体",
        "model_provider_id": bs.chat_provider_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/agents", &req_b, &jwt).await;
    let agent_b_id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing id")
        .to_string();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. Search by "自然语言处理" — Agent A matches literally (FTS5),
    //    Agent B matches via semantic similarity (vector)
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/agents/search",
            &json!({
                "keyword": "自然语言处理",
                "limit": 20,
                "offset": 0
            }),
            &jwt,
        )
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    // Both agents should be found (hybrid: FTS5 for A, vector for B)
    let found_a = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_a_id.as_str()));
    let found_b = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_b_id.as_str()));
    assert!(found_a, "Agent A should be found via FTS5 keyword match");
    assert!(
        found_b,
        "Agent B should be found via vector semantic similarity"
    );

    // Verify ranking: Agent A (FTS5) should rank higher than Agent B (vector only)
    if found_a && found_b {
        let pos_a = items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_a_id.as_str()));
        let pos_b = items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(agent_b_id.as_str()));
        if let (Some(pa), Some(pb)) = (pos_a, pos_b) {
            assert!(
                pa < pb,
                "FTS5 match (Agent A, pos={}) should rank higher than vector match (Agent B, pos={})",
                pa,
                pb
            );
        }
    }

    eprintln!("Hybrid search ranking test passed: FTS5 ranks higher than vector-only match");

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}
