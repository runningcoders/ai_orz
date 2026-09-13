//! 工具与技能向量搜索集成测试
//!
//! 覆盖：
//! - Tool / Skill CRUD 基础流程
//! - Tool / Skill keyword 搜索（FTS5，无 embedding provider 也可工作）
//! - 真实 Embedding provider 场景下的向量语义搜索（ignored，需 API key）
//! - 真实向量索引维护：创建 → 更新 → 删除（ignored）
//!
//! 与 agent_management_test 中的向量测试互补：
//! - agent_management_test 验证 Agent 实体的向量搜索
//! - 本文件验证 Tool / Skill 实体的向量搜索，确认 LanceDB 在多个 collection 上工作正常

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

// ===== 真实向量搜索测试辅助（与 agent_management_test 一致的模式）=====

fn env_or_none(key: &str) -> Option<String> {
    let _ = dotenvy::dotenv();
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

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

struct RealModelConfig {
    embedding_api_key: String,
    embedding_model_name: String,
    embedding_provider_type: &'static str,
    embedding_base_url: Option<String>,
}

impl RealModelConfig {
    fn from_env() -> Option<Self> {
        let embedding_api_key = env_or_none("TEST_EMBEDDING_API_KEY")?;
        let embedding_model_name = env_or_none("TEST_EMBEDDING_MODEL_NAME")
            .unwrap_or_else(|| "text-embedding-3-small".into());
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

async fn create_embedding_provider(app: &TestApp, jwt: &str, cfg: &RealModelConfig) -> String {
    let req = json!({
        "name": format!("TestEmbedding-{}", uuid::Uuid::now_v7()),
        "provider_type": cfg.embedding_provider_type,
        "capability": "Embedding",
        "model_name": cfg.embedding_model_name,
        "api_key": cfg.embedding_api_key,
        "base_url": cfg.embedding_base_url,
        "description": "Real embedding provider for tool/skill vector tests",
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

async fn create_tool(app: &TestApp, jwt: &str, name: &str, description: &str) -> String {
    let req = json!({
        "name": name,
        "description": description,
        "protocol": "Http",
        "config": {
            "method": "GET",
            "url": "https://httpbin.org/get"
        },
        "tags": ["test-tool"],
        "enabled": true,
    });
    let (status, body) = app.post_with_jwt("/api/v1/finance/tools", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing tool id")
        .to_string()
}

async fn update_tool(app: &TestApp, jwt: &str, tool_id: &str, name: &str, description: &str) {
    // UpdateToolRequest 的 `id` 是 String（非 Option），Json 提取器要求 body 包含此字段
    let req = json!({
        "id": tool_id,
        "name": name,
        "description": description,
    });
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/finance/tools/{}", tool_id), &req, jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tool update should succeed"
    );
}

async fn delete_tool(app: &TestApp, jwt: &str, tool_id: &str) {
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/finance/tools/{}", tool_id), jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tool delete should succeed"
    );
}

async fn create_skill(
    app: &TestApp,
    jwt: &str,
    name: &str,
    description: &str,
    tags: Vec<String>,
    status: &str,
) -> String {
    let req = json!({
        "name": name,
        "description": description,
        "tags": tags,
        "status": status,
    });
    let (status, body) = app.post_with_jwt("/api/v1/hr/skills", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing skill id")
        .to_string()
}

async fn update_skill(app: &TestApp, jwt: &str, skill_id: &str, name: &str, description: &str) {
    // UpdateSkillRequest 的 `skill_id` 是 String（非 Option），Json 提取器要求 body 包含此字段
    let req = json!({
        "skill_id": skill_id,
        "name": name,
        "description": description,
    });
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/hr/skills/{}", skill_id), &req, jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "skill update should succeed"
    );
}

async fn delete_skill(app: &TestApp, jwt: &str, skill_id: &str) {
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/hr/skills/{}", skill_id), jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "skill delete should succeed"
    );
}

// =================================================================
// 默认运行的测试（无 embedding provider，FTS5 路径）
// =================================================================

/// Tool CRUD 基础流程：创建 → 查询 → 更新 → 删除
#[sqlx::test]
async fn test_tool_crud(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let tool_name = format!("CrudTool-{}", uuid::Uuid::now_v7());
    let tool_id = create_tool(&app, &jwt, &tool_name, "A test tool for CRUD").await;

    // GET by id
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/finance/tools/{}", tool_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("name").and_then(|v| v.as_str()),
        Some(tool_name.as_str())
    );

    // UPDATE
    let new_name = format!("UpdatedTool-{}", uuid::Uuid::now_v7());
    update_tool(&app, &jwt, &tool_id, &new_name, "Updated description").await;

    // Verify update
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/finance/tools/{}", tool_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("name").and_then(|v| v.as_str()),
        Some(new_name.as_str())
    );

    // DELETE
    delete_tool(&app, &jwt, &tool_id).await;

    // GET after delete → 404
    let (status, _body) = app
        .get_with_jwt(&format!("/api/v1/finance/tools/{}", tool_id), &jwt)
        .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

    let _ = bs; // 抑制未使用警告
}

/// Skill CRUD 基础流程：创建 → 查询 → 更新 → 删除
#[sqlx::test]
async fn test_skill_crud(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let skill_name = format!("CrudSkill-{}", uuid::Uuid::now_v7());
    let skill_id = create_skill(
        &app,
        &jwt,
        &skill_name,
        "A test skill for CRUD",
        vec!["test-skill".to_string()],
        "Published",
    )
    .await;

    // GET by id
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/skills/{}", skill_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("name").and_then(|v| v.as_str()),
        Some(skill_name.as_str())
    );

    // UPDATE
    let new_name = format!("UpdatedSkill-{}", uuid::Uuid::now_v7());
    update_skill(&app, &jwt, &skill_id, &new_name, "Updated description").await;

    // Verify update
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/skills/{}", skill_id), &jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("name").and_then(|v| v.as_str()),
        Some(new_name.as_str())
    );

    // DELETE
    delete_skill(&app, &jwt, &skill_id).await;

    // Note: Skill delete 可能是软删除，这里只验证删除操作本身成功
    // 删除后的状态由 test_real_skill_vector_maintenance 在向量搜索层面验证
}

/// Tool FTS5 keyword 搜索（无 embedding provider，仅走 FTS5 路径）
#[sqlx::test]
async fn test_tool_fts5_search(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let unique = uuid::Uuid::now_v7().to_string();
    let name_a = format!("FtsTool-{}", unique);
    let name_b = format!("OtherTool-{}", unique);

    let _id_a = create_tool(&app, &jwt, &name_a, "负责自然语言处理和文本分析的内置工具").await;
    let _id_b = create_tool(&app, &jwt, &name_b, "负责数据库管理和SQL查询优化的工具").await;

    // Search by keyword contained in tool A's description
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "自然语言", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found_a = items
        .iter()
        .any(|i| i.get("name").and_then(|v| v.as_str()) == Some(name_a.as_str()));
    assert!(found_a, "tool A should be found via FTS5 keyword match");
}

/// Skill FTS5 keyword 搜索（无 embedding provider，仅走 FTS5 路径）
#[sqlx::test]
async fn test_skill_fts5_search(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let unique = uuid::Uuid::now_v7().to_string();
    let name_a = format!("FtsSkill-{}", unique);
    let name_b = format!("OtherSkill-{}", unique);

    let _id_a = create_skill(
        &app,
        &jwt,
        &name_a,
        "负责自然语言处理和文本分析的技能",
        vec!["nlp".to_string()],
        "Published",
    )
    .await;
    let _id_b = create_skill(
        &app,
        &jwt,
        &name_b,
        "负责数据库管理和SQL查询优化的技能",
        vec!["db".to_string()],
        "Published",
    )
    .await;

    // Search by keyword contained in skill A's description
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/skills/search",
            &json!({"keyword": "自然语言", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found_a = items
        .iter()
        .any(|i| i.get("name").and_then(|v| v.as_str()) == Some(name_a.as_str()));
    assert!(found_a, "skill A should be found via FTS5 keyword match");
}

// =================================================================
// 真实向量搜索测试（ignored，需 API key + 真实 LanceDB）
// =================================================================

/// 真实 Tool 向量语义搜索：创建 embedding provider → 创建语义相关工具
/// → 用未出现在描述中但语义相关的关键词搜索 → 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_tool_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping tool vector test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建 embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. 创建一个工具，描述中含"深度学习"但不出现"神经网络"
    let tool_name = format!("VectorTool-{}", uuid::Uuid::now_v7());
    let tool_id = create_tool(
        &app,
        &jwt,
        &tool_name,
        "这是一个专门负责深度学习模型训练与梯度下降优化的工具",
    )
    .await;

    // 等待向量索引完成
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. 用语义相关但不出现的关键词"神经网络"搜索
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "神经网络", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_id.as_str()));
    assert!(
        found,
        "tool should be found via semantic vector search for '神经网络' \
         (description mentions '深度学习'); items: {:?}",
        items
            .iter()
            .map(|i| i.get("name").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Tool vector semantic search test passed");

    // 清理
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

/// 真实 Tool 向量索引维护：创建 → 更新 → 验证新描述生效 → 删除 → 验证不再被搜索到
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_tool_vector_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping tool maintenance test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 1. 创建工具，描述为"前端页面开发"
    let tool_name = format!("MaintTool-{}", uuid::Uuid::now_v7());
    let tool_id = create_tool(
        &app,
        &jwt,
        &tool_name,
        "负责前端页面开发和用户界面设计的工具",
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 2. 验证"前端"关键词能搜到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "前端", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_before = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_id.as_str()));
    assert!(found_before, "tool should be found before update");

    // 3. 更新描述为"数据库管理"
    let new_name = format!("MaintToolUpdated-{}", uuid::Uuid::now_v7());
    update_tool(
        &app,
        &jwt,
        &tool_id,
        &new_name,
        "负责数据库管理和SQL查询优化的工具",
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 4. 验证"数据库"关键词能搜到（新向量已生效）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "数据库", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after_update = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_id.as_str()));
    assert!(
        found_after_update,
        "tool should be found via new description after update"
    );

    // 5. 删除工具
    delete_tool(&app, &jwt, &tool_id).await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 6. 再次搜索"数据库"，应该不再出现
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "数据库", "limit": 50, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after_delete = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_id.as_str()));
    assert!(!found_after_delete, "tool should NOT be found after delete");

    eprintln!("Tool vector index maintenance test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

/// 真实 Skill 向量语义搜索：创建 embedding provider → 创建语义相关技能
/// → 用未出现在描述中但语义相关的关键词搜索 → 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_skill_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping skill vector test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 1. 创建技能，描述含"机器学习"但不出现"人工智能"
    let skill_name = format!("VectorSkill-{}", uuid::Uuid::now_v7());
    let skill_id = create_skill(
        &app,
        &jwt,
        &skill_name,
        "这是一个负责机器学习模型训练与特征工程的技能",
        vec!["ml".to_string()],
        "Published",
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 2. 用"人工智能"搜索（语义相关，未出现在描述中）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/skills/search",
            &json!({"keyword": "人工智能", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(skill_id.as_str()));
    assert!(
        found,
        "skill should be found via semantic vector search for '人工智能' \
         (description mentions '机器学习'); items: {:?}",
        items
            .iter()
            .map(|i| i.get("name").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Skill vector semantic search test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

/// 真实 Skill 向量索引维护：创建 → 更新 → 验证新描述生效 → 删除 → 验证不再被搜索到
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_skill_vector_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping skill maintenance test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 1. 创建技能，描述为"前端页面开发"
    let skill_name = format!("MaintSkill-{}", uuid::Uuid::now_v7());
    let skill_id = create_skill(
        &app,
        &jwt,
        &skill_name,
        "负责前端页面开发和用户界面设计的技能",
        vec!["frontend".to_string()],
        "Published",
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 2. 验证"前端"关键词能搜到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/skills/search",
            &json!({"keyword": "前端", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_before = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(skill_id.as_str()));
    assert!(found_before, "skill should be found before update");

    // 3. 更新描述为"数据库管理"
    let new_name = format!("MaintSkillUpdated-{}", uuid::Uuid::now_v7());
    update_skill(
        &app,
        &jwt,
        &skill_id,
        &new_name,
        "负责数据库管理和SQL查询优化的技能",
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 4. 验证"数据库"关键词能搜到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/skills/search",
            &json!({"keyword": "数据库", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after_update = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(skill_id.as_str()));
    assert!(
        found_after_update,
        "skill should be found via new description after update"
    );

    // 5. 删除技能
    delete_skill(&app, &jwt, &skill_id).await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 6. 再次搜索"数据库"，应该不再出现
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/hr/skills/search",
            &json!({"keyword": "数据库", "limit": 50, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after_delete = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(skill_id.as_str()));
    assert!(
        !found_after_delete,
        "skill should NOT be found after delete"
    );

    eprintln!("Skill vector index maintenance test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

/// 真实混合搜索排序：同时创建 FTS5 匹配和向量匹配的 Tool/Skill，
/// 验证 FTS5 命中排名高于向量命中（与 Agent 的对应测试一致）
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_tool_skill_hybrid_ranking(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping hybrid ranking test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let unique = uuid::Uuid::now_v7().to_string();

    // Tool A: 描述包含关键词"自然语言处理"（FTS5 命中）
    let tool_a_name = format!("FtsTool-{}", unique);
    let tool_a_id = create_tool(
        &app,
        &jwt,
        &tool_a_name,
        "专注于自然语言处理和文本分析的工具",
    )
    .await;

    // Tool B: 描述语义相关但不包含关键词（向量命中）
    let tool_b_name = format!("VectorTool-{}", unique);
    let tool_b_id = create_tool(&app, &jwt, &tool_b_name, "负责语义理解和文本挖掘的工具").await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 搜索"自然语言处理"：Tool A 应 FTS5 命中，Tool B 应向量命中
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/search",
            &json!({"keyword": "自然语言处理", "limit": 20, "offset": 0}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found_a = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_a_id.as_str()));
    let found_b = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_b_id.as_str()));
    assert!(found_a, "Tool A should be found via FTS5");
    assert!(found_b, "Tool B should be found via vector");

    // FTS5 命中应排名更高
    if let (Some(pos_a), Some(pos_b)) = (
        items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_a_id.as_str())),
        items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(tool_b_id.as_str())),
    ) {
        assert!(
            pos_a < pos_b,
            "FTS5 match (Tool A, pos={}) should rank higher than vector match (Tool B, pos={})",
            pos_a,
            pos_b
        );
    }

    eprintln!("Tool hybrid search ranking test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

// =================================================================
// pre-filter 谓词下推（手工向量直写 + DAO 搜索路径，无需 embedding）
// =================================================================

/// 手工构造向量索引参数（绕开真实 embedding，直写 vector store）
fn manual_vector_params(
    vector: Vec<f32>,
    payload: ai_orz::models::vector::VectorPayload,
) -> ai_orz::models::vector::VectorIndexParams {
    let payload_hash = payload.hash();
    ai_orz::models::vector::VectorIndexParams {
        vector,
        content_hash: sha256::digest(uuid::Uuid::now_v7().to_string()),
        payload,
        payload_hash,
        model_provider_id: "manual-test-provider".to_string(),
        embedding_model: "manual-test-model".to_string(),
        expire_at: None,
    }
}

/// pre-filter 谓词下推：Tool 语义搜索不被禁用（Disabled）工具稀释 Top-K
///
/// 数据布局（query=[1,0]，余弦距离）：
/// - Disabled 工具全局最近（distance=0）：若为 post-filter（先取 Top-K 再过滤），
///   exclude_status=Disabled 视角 top_k=1 会取到它再被过滤掉 → 返回空；
///   正确的 pre-filter 应在满足谓词的候选集内选取 → 命中 Enabled 工具。
#[sqlx::test]
async fn test_tool_vector_search_prefilter_status(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    let disabled_id = format!("tl_pf_{}", uuid::Uuid::now_v7());
    let enabled_id = format!("tl_pf_{}", uuid::Uuid::now_v7());

    let store = ctx.vector_store();
    store
        .upsert(
            "tools",
            &disabled_id,
            &manual_vector_params(
                vec![1.0, 0.0],
                ai_orz::models::vector::VectorPayload {
                    status: Some(::common::enums::ToolStatus::Disabled.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert disabled tool failed");
    store
        .upsert(
            "tools",
            &enabled_id,
            &manual_vector_params(
                vec![0.6, 0.8],
                ai_orz::models::vector::VectorPayload {
                    status: Some(::common::enums::ToolStatus::Enabled.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert enabled tool failed");

    let dao = ai_orz::service::dao::tool::new_tool_vector_dao();
    let query_vector: Vec<f32> = vec![1.0, 0.0];

    // 基线：无 filter 时全局最近的是禁用工具（证明数据已落库且距离排序生效）
    let hits = dao
        .search_vector(
            ctx.clone(),
            &query_vector,
            1,
            &ai_orz::service::dao::tool::ToolQuery::default(),
        )
        .await
        .expect("search without filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, disabled_id);

    // exclude_status=Disabled：top_k=1 必须命中启用工具（post-filter 实现会返回空）
    let exclude_query = ai_orz::service::dao::tool::ToolQuery {
        exclude_status: Some(::common::enums::ToolStatus::Disabled),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &exclude_query)
        .await
        .expect("search with exclude_status failed");
    assert_eq!(
        hits.len(),
        1,
        "pre-filter 后 top-1 必须命中（post-filter 实现会返回空）"
    );
    assert_eq!(hits[0].row.id, enabled_id);
    assert_eq!(
        hits[0].row.payload.status.as_deref(),
        Some(
            ::common::enums::ToolStatus::Enabled
                .to_i32()
                .to_string()
                .as_str()
        )
    );

    // 正向 status=Enabled 下推：命中启用工具
    let status_query = ai_orz::service::dao::tool::ToolQuery {
        status: Some(::common::enums::ToolStatus::Enabled),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &status_query)
        .await
        .expect("search with status failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, enabled_id);

    // 放大 top_k：禁用工具不应出现
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 5, &exclude_query)
        .await
        .expect("search with exclude_status (top 5) failed");
    assert!(!hits.is_empty());
    assert!(
        !hits.iter().any(|h| h.row.id == disabled_id),
        "状态排除：Disabled 工具不应出现"
    );
}

/// pre-filter 谓词下推：Skill 语义搜索按 category 隔离，Top-K 不被其他分类稀释
///
/// 数据布局（query=[1,0]，余弦距离）：
/// - coding 类技能全局最近（distance=0）：math 视角若为 post-filter 会返回空
#[sqlx::test]
async fn test_skill_vector_search_prefilter_category(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    let coding_id = format!("sk_pf_{}", uuid::Uuid::now_v7());
    let math_id = format!("sk_pf_{}", uuid::Uuid::now_v7());

    let store = ctx.vector_store();
    store
        .upsert(
            "skills",
            &coding_id,
            &manual_vector_params(
                vec![1.0, 0.0],
                ai_orz::models::vector::VectorPayload {
                    entity_type: Some("coding".to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert coding skill failed");
    store
        .upsert(
            "skills",
            &math_id,
            &manual_vector_params(
                vec![0.6, 0.8],
                ai_orz::models::vector::VectorPayload {
                    entity_type: Some("math".to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert math skill failed");

    let dao = ai_orz::service::dao::skill::new_skill_vector_dao();
    let query_vector: Vec<f32> = vec![1.0, 0.0];

    // 基线：无 filter 时全局最近的是 coding 类技能
    let hits = dao
        .search_vector(
            ctx.clone(),
            &query_vector,
            1,
            &ai_orz::service::dao::skill::SkillQuery::default(),
        )
        .await
        .expect("search without filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, coding_id);

    // category 下推：math 视角 top_k=1 必须命中自己的技能（post-filter 会返回空）
    let category_query = ai_orz::service::dao::skill::SkillQuery {
        category: Some("math".to_string()),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &category_query)
        .await
        .expect("search with category filter failed");
    assert_eq!(
        hits.len(),
        1,
        "pre-filter 后 top-1 必须命中（post-filter 实现会返回空）"
    );
    assert_eq!(hits[0].row.id, math_id);
    assert_eq!(hits[0].row.payload.entity_type.as_deref(), Some("math"));

    // 放大 top_k：coding 类技能不应出现
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 5, &category_query)
        .await
        .expect("search with category filter (top 5) failed");
    assert!(
        !hits.iter().any(|h| h.row.id == coding_id),
        "category 过滤：coding 类技能不应出现"
    );
}
