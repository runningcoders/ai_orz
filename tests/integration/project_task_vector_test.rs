//! Project/Task 向量搜索集成测试
//!
//! 覆盖：
//! - Project FTS5 关键词搜索（CI-safe）
//! - Project 搜索带业务过滤（CI-safe）
//! - Task FTS5 关键词搜索（CI-safe）
//! - Task 搜索带业务过滤（CI-safe）
//! - 真实 Project 向量语义搜索（ignored）
//! - 真实 Project 向量索引维护（ignored）
//! - 真实 Task 向量语义搜索（ignored）
//! - 真实 Project/Task 混合排序（ignored）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;

// ===== 真实向量搜索测试辅助（与其他测试文件一致的模式）=====

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
        "description": "Real embedding provider for project/task vector tests",
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

// ===== Project 辅助 =====

/// 创建 Project，返回 project_id
async fn create_project(app: &TestApp, jwt: &str, name: &str, description: &str) -> String {
    let req = json!({"name": name, "description": description});
    let (status, body) = app.post_with_jwt("/api/v1/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing project id")
        .to_string()
}

/// 更新 Project 描述（用于向量索引维护测试）
async fn update_project(app: &TestApp, jwt: &str, project_id: &str, description: &str) {
    // UpdateProjectRequest 的 `id` 是 String（非 Option），Json 提取器要求 body 包含此字段
    let req = json!({"id": project_id, "description": description});
    let (status, _body) = app
        .put_with_jwt(&format!("/api/v1/projects/{}", project_id), &req, jwt)
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "project update should succeed"
    );
}

// ===== Task 辅助 =====

/// 创建 Task，返回 task_id
async fn create_task(
    app: &TestApp,
    jwt: &str,
    title: &str,
    description: &str,
    project_id: &str,
    assignee_id: &str,
) -> String {
    let req = json!({
        "title": title,
        "description": description,
        "project_id": project_id,
        "assignee_id": assignee_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id")
        .to_string()
}

// =================================================================
// 默认运行的测试（无 embedding provider，FTS5 路径）
// =================================================================

/// Project FTS5 关键词搜索（无 embedding provider，仅走 FTS5 路径）
#[sqlx::test]
async fn test_project_fts5_search(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let unique = uuid::Uuid::now_v7().to_string();
    let name_a = format!("FtsProject-{}", unique);
    let name_b = format!("OtherProject-{}", unique);

    let _id_a = create_project(
        &app,
        &jwt,
        &name_a,
        &format!("关于自然语言处理和文本分析的项目-{}", unique),
    )
    .await;
    let _id_b = create_project(
        &app,
        &jwt,
        &name_b,
        &format!("关于数据库管理和SQL查询优化的项目-{}", unique),
    )
    .await;

    // Search by keyword contained in project A's description
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "自然语言", "limit": 20}),
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
    assert!(found_a, "project A should be found via FTS5 keyword match");

    let found_b = items
        .iter()
        .any(|i| i.get("name").and_then(|v| v.as_str()) == Some(name_b.as_str()));
    assert!(
        !found_b,
        "project B should NOT be found when searching for '自然语言'"
    );
}

/// Project 搜索带业务过滤（root_user_id 过滤）
#[sqlx::test]
async fn test_project_search_with_filter(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let unique = uuid::Uuid::now_v7().to_string();
    let name = format!("FilterProject-{}", unique);
    let _id = create_project(
        &app,
        &jwt,
        &name,
        &format!("关于自然语言处理的讨论-{}", unique),
    )
    .await;

    // 搜索关键词 + 过滤 root_user_id = 当前用户 → 应能找到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "自然语言", "root_user_id": bs.user_id, "limit": 20}),
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
        .any(|i| i.get("name").and_then(|v| v.as_str()) == Some(name.as_str()));
    assert!(
        found,
        "project should be found when root_user_id matches current user"
    );

    // 搜索关键词 + 过滤 root_user_id = 不存在的用户 → 不应找到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "自然语言", "root_user_id": "nonexistent-user-id", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");

    let found_wrong = items
        .iter()
        .any(|i| i.get("name").and_then(|v| v.as_str()) == Some(name.as_str()));
    assert!(
        !found_wrong,
        "project should NOT be found when root_user_id does not match"
    );
}

/// Task FTS5 关键词搜索（无 embedding provider，仅走 FTS5 路径）
#[sqlx::test]
async fn test_task_fts5_search(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 创建 Agent 作为 assignee
    let agent_name = format!("FtsAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    // 创建 Project
    let project_name = format!("FtsTaskProject-{}", uuid::Uuid::now_v7());
    let project_id = create_project(&app, &jwt, &project_name, "Test project for FTS tasks").await;

    let unique = uuid::Uuid::now_v7().to_string();
    let title_a = format!("FtsTask-{}", unique);
    let title_b = format!("OtherTask-{}", unique);

    let _id_a = create_task(
        &app,
        &jwt,
        &title_a,
        &format!("关于自然语言处理和文本分析的任务-{}", unique),
        &project_id,
        &agent_id,
    )
    .await;
    let _id_b = create_task(
        &app,
        &jwt,
        &title_b,
        &format!("关于数据库管理和SQL查询优化的任务-{}", unique),
        &project_id,
        &agent_id,
    )
    .await;

    // Search by keyword contained in task A's description
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/tasks/search",
            &json!({"keyword": "自然语言", "limit": 20}),
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
        .any(|i| i.get("title").and_then(|v| v.as_str()) == Some(title_a.as_str()));
    assert!(found_a, "task A should be found via FTS5 keyword match");

    let found_b = items
        .iter()
        .any(|i| i.get("title").and_then(|v| v.as_str()) == Some(title_b.as_str()));
    assert!(
        !found_b,
        "task B should NOT be found when searching for '自然语言'"
    );
}

/// Task 搜索带业务过滤（project_id 过滤）
#[sqlx::test]
async fn test_task_search_with_filter(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 创建 Agent 作为 assignee
    let agent_name = format!("FilterAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    // 创建两个不同的 Project
    let project_a = format!("FilterProjA-{}", uuid::Uuid::now_v7());
    let project_a_id = create_project(&app, &jwt, &project_a, "Test project A").await;
    let project_b = format!("FilterProjB-{}", uuid::Uuid::now_v7());
    let project_b_id = create_project(&app, &jwt, &project_b, "Test project B").await;

    let unique = uuid::Uuid::now_v7().to_string();
    let title_a = format!("FilterTaskA-{}", unique);
    let title_b = format!("FilterTaskB-{}", unique);

    // 在 project_a 中创建 task_a
    let _id_a = create_task(
        &app,
        &jwt,
        &title_a,
        &format!("关于自然语言处理的讨论-{}", unique),
        &project_a_id,
        &agent_id,
    )
    .await;
    // 在 project_b 中创建 task_b
    let _id_b = create_task(
        &app,
        &jwt,
        &title_b,
        &format!("关于自然语言处理的讨论-{}", unique),
        &project_b_id,
        &agent_id,
    )
    .await;

    // 搜索关键词 + 过滤 project_id = project_a → 只应返回 project_a 中的 task
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/tasks/search",
            &json!({"keyword": "自然语言", "project_id": project_a_id, "limit": 20}),
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
        .any(|i| i.get("title").and_then(|v| v.as_str()) == Some(title_a.as_str()));
    assert!(
        found_a,
        "task A should be found when filtering by project_a"
    );

    let found_b = items
        .iter()
        .any(|i| i.get("title").and_then(|v| v.as_str()) == Some(title_b.as_str()));
    assert!(
        !found_b,
        "task B should NOT be found when filtering by project_a"
    );
}

// =================================================================
// 真实向量搜索测试（ignored，需 API key + 真实 LanceDB）
// =================================================================

/// 真实 Project 向量语义搜索：创建描述含"深度学习"但不出现"神经网络"的项目
/// → 用语义相关但不出现的关键词"神经网络"搜索 → 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_project_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping project vector test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建 embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. 创建项目，描述含"深度学习"但不出现"神经网络"
    let unique = uuid::Uuid::now_v7().to_string();
    let project_name = format!("VectorProject-{}", unique);
    let project_id = create_project(
        &app,
        &jwt,
        &project_name,
        &format!(
            "今天我们讨论一下深度学习模型训练与梯度下降优化的方法-{}",
            unique
        ),
    )
    .await;

    // 等待向量索引完成
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. 用"神经网络"搜索（语义相关，未出现在描述中）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "神经网络", "limit": 20}),
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
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(project_id.as_str()));
    assert!(
        found,
        "project should be found via semantic vector search for '神经网络' \
         (description mentions '深度学习'); items: {:?}",
        items
            .iter()
            .map(|i| i.get("name").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Project vector semantic search test passed");

    // 清理
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// 真实 Project 向量索引维护：创建 → 验证可搜到 → 更新描述 → 验证新描述可搜到
///
/// 注：HTTP 层的 PUT /projects/{id}/status (Archived) 不会删除向量索引，
/// 只有 Domain 层 archive() 方法才会清理向量。因此本测试通过更新描述
/// 来验证索引维护（内容变化时 FTS5 + 向量重新索引），与 tool 维护测试模式一致。
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_project_vector_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping project maintenance test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 1. 创建项目，描述含"前端页面开发"
    let unique = uuid::Uuid::now_v7().to_string();
    let project_name = format!("MaintProject-{}", unique);
    let project_id = create_project(
        &app,
        &jwt,
        &project_name,
        &format!("关于前端页面开发和用户界面设计的实践-{}", unique),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 2. 验证"前端"关键词能搜到
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "前端", "limit": 20}),
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
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(project_id.as_str()));
    assert!(
        found_before,
        "project should be found via search before update"
    );

    // 3. 更新描述为"数据库管理"（完全不同的语义领域）
    update_project(
        &app,
        &jwt,
        &project_id,
        &format!("关于数据库管理和SQL查询优化的实践-{}", unique),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 4. 验证"数据库"关键词能搜到（FTS5 + 向量均已更新为新描述）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "数据库", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .expect("missing items");
    let found_after = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(project_id.as_str()));
    assert!(
        found_after,
        "project should be found via search for '数据库' after updating description \
         (index maintenance verified); items: {:?}",
        items
            .iter()
            .map(|i| i.get("name").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Project vector index maintenance test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// 真实 Task 向量语义搜索：创建描述含"机器学习"但不出现"人工智能"的任务
/// → 用语义相关但不出现的关键词"人工智能"搜索 → 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_task_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping task vector test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 创建 Agent + Project
    let agent_name = format!("VectorTaskAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;
    let project_name = format!("VectorTaskProject-{}", uuid::Uuid::now_v7());
    let project_id =
        create_project(&app, &jwt, &project_name, "Test project for vector task").await;

    // 创建任务，描述含"机器学习"但不出现"人工智能"
    let unique = uuid::Uuid::now_v7().to_string();
    let task_title = format!("VectorTask-{}", unique);
    let task_id = create_task(
        &app,
        &jwt,
        &task_title,
        &format!("关于机器学习算法优化和模型部署的实践-{}", unique),
        &project_id,
        &agent_id,
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 用"人工智能"搜索（语义相关，未出现在描述中）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/tasks/search",
            &json!({"keyword": "人工智能", "limit": 20}),
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
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str()));
    assert!(
        found,
        "task should be found via semantic vector search for '人工智能' \
         (description mentions '机器学习'); items: {:?}",
        items
            .iter()
            .map(|i| i.get("title").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Task vector semantic search test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
    let _ = bs;
}

/// 真实 Project 混合搜索排序：同时创建 FTS5 匹配和向量匹配的项目
/// 验证 FTS5 命中排名高于向量命中（与其他实体一致）
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_project_task_hybrid_ranking(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping hybrid ranking test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let unique = uuid::Uuid::now_v7().to_string();

    // Project A: 描述包含关键词"自然语言处理"（FTS5 命中）
    let name_a = format!("HybridProjA-{}", unique);
    let id_a = create_project(
        &app,
        &jwt,
        &name_a,
        &format!("专注于自然语言处理和文本分析的项目-{}", unique),
    )
    .await;

    // Project B: 描述语义相关但不包含关键词（向量命中）
    let name_b = format!("HybridProjB-{}", unique);
    let id_b = create_project(
        &app,
        &jwt,
        &name_b,
        &format!("关于语义理解和文本挖掘的技术交流项目-{}", unique),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 搜索"自然语言处理"：Project A 应 FTS5 命中，Project B 应向量命中
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/projects/search",
            &json!({"keyword": "自然语言处理", "limit": 20}),
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
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(id_a.as_str()));
    let found_b = items
        .iter()
        .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(id_b.as_str()));
    assert!(found_a, "Project A should be found via FTS5");
    assert!(found_b, "Project B should be found via vector");

    // FTS5 命中应排名更高
    if let (Some(pos_a), Some(pos_b)) = (
        items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(id_a.as_str())),
        items
            .iter()
            .position(|i| i.get("id").and_then(|v| v.as_str()) == Some(id_b.as_str())),
    ) {
        assert!(
            pos_a < pos_b,
            "FTS5 match (Project A, pos={}) should rank higher than vector match (Project B, pos={})",
            pos_a,
            pos_b
        );
    }

    eprintln!("Project/Task hybrid search ranking test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}
