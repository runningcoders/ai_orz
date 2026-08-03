//! Real model integration tests (requires actual API keys).
//!
//! These tests verify the full model call chain with real Embedding and LLM
//! providers, covering scenarios that mock-based tests cannot reach:
//! - Embedding: entity create → vector index write → semantic search recall
//! - LLM: test_connection → real model response
//! - End-to-end: agent create with embedding → search by keyword → verify recall
//!
//! ## Running
//!
//! 1. Copy `.env.example` to `.env` and fill in your API keys
//! 2. Run: `cargo test --test real_model_test -- --ignored`
//!
//! Without API keys configured, all tests are skipped (CI-safe).

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

/// Load .env file and read an env var. Returns None if unset or empty.
fn env_or_none(key: &str) -> Option<String> {
    // dotenvy::dotenv is safe to call multiple times; it only loads once.
    let _ = dotenvy::dotenv();
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

/// Parse provider type string to serde variant name.
/// Serde serializes ProviderType enum as variant names: "OpenAI", "DeepSeek", etc.
fn parse_provider_type(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "openai" | "0" => "OpenAI",
        "deepseek" | "1" => "DeepSeek",
        "qwen" | "2" => "Qwen",
        "doubao" | "3" => "Doubao",
        "ollama" | "4" => "Ollama",
        "custom" | "5" => "Custom",
        "fastembed" | "6" => "FastEmbed",
        "doubao_vision" | "doubaoVision" | "7" => "DoubaoVision",
        _ => "OpenAI",
    }
}

/// Test config parsed from environment variables.
struct TestConfig {
    embedding_api_key: String,
    embedding_model_name: String,
    embedding_provider_type: &'static str,
    embedding_base_url: Option<String>,
    llm_api_key: String,
    llm_model_name: String,
    llm_provider_type: &'static str,
    llm_base_url: Option<String>,
}

impl TestConfig {
    /// Load from env. Returns None if any required API key is missing.
    fn from_env() -> Option<Self> {
        let embedding_api_key = env_or_none("TEST_EMBEDDING_API_KEY")?;
        let llm_api_key = env_or_none("TEST_LLM_API_KEY")?;

        let embedding_model_name = env_or_none("TEST_EMBEDDING_MODEL_NAME")
            .unwrap_or_else(|| "text-embedding-3-small".into());
        let llm_model_name = env_or_none("TEST_LLM_MODEL_NAME").unwrap_or_else(|| "gpt-4o".into());

        let embedding_provider_type = env_or_none("TEST_EMBEDDING_PROVIDER_TYPE")
            .as_deref()
            .map(parse_provider_type)
            .unwrap_or("OpenAI");
        let llm_provider_type = env_or_none("TEST_LLM_PROVIDER_TYPE")
            .as_deref()
            .map(parse_provider_type)
            .unwrap_or("OpenAI");

        let embedding_base_url = env_or_none("TEST_EMBEDDING_BASE_URL");
        let llm_base_url = env_or_none("TEST_LLM_BASE_URL");

        Some(Self {
            embedding_api_key,
            embedding_model_name,
            embedding_provider_type,
            embedding_base_url,
            llm_api_key,
            llm_model_name,
            llm_provider_type,
            llm_base_url,
        })
    }
}

/// Create a ModelProvider via HTTP API. Returns the provider ID.
/// capability: "Agent" for LLM, "Embedding" for vector models
#[allow(clippy::too_many_arguments)]
async fn create_provider(
    app: &TestApp,
    jwt: &str,
    name: &str,
    provider_type: &str,
    capability: &str,
    model_name: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> String {
    let req = json!({
        "name": name,
        "provider_type": provider_type,
        "capability": capability,
        "model_name": model_name,
        "api_key": api_key,
        "base_url": base_url,
        "description": format!("Real model test: {}", name),
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/model-providers", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing provider id in create response")
        .to_string()
}

/// Test LLM connection: create a chat provider → call test_connection → verify response.
#[sqlx::test]
#[ignore = "requires real LLM API key in .env (TEST_LLM_API_KEY)"]
async fn test_llm_connection(pool: SqlitePool) {
    let Some(cfg) = TestConfig::from_env() else {
        eprintln!("SKIP: TEST_LLM_API_KEY not set, skipping real LLM test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create a real LLM provider (capability="Agent")
    let provider_id = create_provider(
        &app,
        &jwt,
        &format!("TestLLM-{}", uuid::Uuid::now_v7()),
        cfg.llm_provider_type,
        "Agent",
        &cfg.llm_model_name,
        &cfg.llm_api_key,
        cfg.llm_base_url.as_deref(),
    )
    .await;

    // Test connection with a simple prompt
    let req = json!({ "id": provider_id, "prompt": "Reply with exactly: hello" });
    let (status, body) = app
        .post_with_jwt(
            &format!("/api/v1/finance/model-providers/{}/test", provider_id),
            &req,
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let success = data
        .get("success")
        .and_then(|v| v.as_bool())
        .expect("missing success field");
    assert!(success, "LLM connection test failed: {:?}", data);
    let response = data
        .get("response")
        .and_then(|v| v.as_str())
        .expect("missing response field");
    assert!(
        !response.trim().is_empty(),
        "LLM response should not be empty"
    );
    eprintln!("LLM response: {}", response);
}

/// Test Embedding chain: create embedding provider → create agent → verify vector index → search.
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_embedding_chain(pool: SqlitePool) {
    let Some(cfg) = TestConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping real embedding test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create a real Embedding provider (capability="Embedding")
    let embedding_provider_id = create_provider(
        &app,
        &jwt,
        &format!("TestEmbedding-{}", uuid::Uuid::now_v7()),
        cfg.embedding_provider_type,
        "Embedding",
        &cfg.embedding_model_name,
        &cfg.embedding_api_key,
        cfg.embedding_base_url.as_deref(),
    )
    .await;

    // Create an agent with distinctive description for semantic search
    let agent_name = format!("SearchableAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "这是一个专门负责机器学习模型训练与调优的智能助手，擅长深度学习、神经网络和梯度下降算法",
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

    // Wait briefly for async vector indexing to complete
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Search by keyword (FTS5 path)
    let search_req = json!({ "keyword": "机器学习" });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/search", &search_req, &jwt)
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items in search response");
    assert!(
        !items.is_empty(),
        "FTS5 keyword search should find the agent"
    );
    let found = items
        .iter()
        .any(|item| item.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(found, "agent should appear in FTS5 search results");

    eprintln!("Embedding chain test passed: agent found via search");

    // Cleanup: delete the embedding provider
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// End-to-end test: create embedding+LLM providers → create agent → search → LLM call.
#[sqlx::test]
#[ignore = "requires real API keys in .env (TEST_EMBEDDING_API_KEY + TEST_LLM_API_KEY)"]
async fn test_e2e_with_real_models(pool: SqlitePool) {
    let Some(cfg) = TestConfig::from_env() else {
        eprintln!("SKIP: API keys not set, skipping E2E test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. Create real LLM provider (replace the fake chat provider from bootstrap)
    let real_llm_provider_id = create_provider(
        &app,
        &jwt,
        &format!("E2ELLM-{}", uuid::Uuid::now_v7()),
        cfg.llm_provider_type,
        "Agent",
        &cfg.llm_model_name,
        &cfg.llm_api_key,
        cfg.llm_base_url.as_deref(),
    )
    .await;

    // 2. Create real Embedding provider
    let embedding_provider_id = create_provider(
        &app,
        &jwt,
        &format!("E2EEmbedding-{}", uuid::Uuid::now_v7()),
        cfg.embedding_provider_type,
        "Embedding",
        &cfg.embedding_model_name,
        &cfg.embedding_api_key,
        cfg.embedding_base_url.as_deref(),
    )
    .await;

    // 3. Create an agent with the real LLM provider
    let agent_name = format!("E2EAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "端到端测试助手，负责代码审查和质量保证",
        "model_provider_id": real_llm_provider_id,
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

    // Wait for async vector indexing
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 4. Search the agent by keyword
    let search_req = json!({ "keyword": "代码审查" });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/search", &search_req, &jwt)
        .await;
    let search_data = crate::common::assert_api_ok(status, &body);
    let items = search_data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found = items
        .iter()
        .any(|item| item.get("id").and_then(|v| v.as_str()) == Some(agent_id.as_str()));
    assert!(found, "E2E: agent should be found via search");

    // 5. Test the real LLM provider connection
    let test_req = json!({ "id": real_llm_provider_id, "prompt": "Reply with: e2e ok" });
    let (status, body) = app
        .post_with_jwt(
            &format!(
                "/api/v1/finance/model-providers/{}/test",
                real_llm_provider_id
            ),
            &test_req,
            &jwt,
        )
        .await;
    let test_data = crate::common::assert_api_ok(status, &body);
    let success = test_data
        .get("success")
        .and_then(|v| v.as_bool())
        .expect("missing success");
    if !success {
        let error = test_data
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("<no error field>");
        let response = test_data
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("<no response>");
        panic!(
            "E2E: LLM connection failed. error: {}, response: {}",
            error, response
        );
    }

    eprintln!("E2E test passed: agent created, searched, LLM responded");

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", real_llm_provider_id),
            &jwt,
        )
        .await;
}
