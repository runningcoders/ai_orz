//! Memory 集成测试
//!
//! 覆盖：
//! - Part A（CI 默认）：短期记忆 query / task_id 过滤 / FTS5 关键词搜索 / 知识节点搜索
//! - Part B（#[ignore]）：真实向量语义搜索 / 索引维护 / 混合排序
//!
//! 数据准备策略：
//! - 记忆创建直接调用 domain layer（runtime_domain().memory().create()）
//!   原因：HTTP handler 的 save_short_term_memory / create_memory 依赖 ctx.agent_id()，
//!   该字段在 HTTP 调用时为空（无 X-Agent-Id header），无法为指定 agent 创建记忆。
//! - 查询/搜索通过 HTTP 端点验证（POST /api/v1/hr/agents/query_memory / search_memory）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::enums::MemoryStatus;
use ai_orz::models::memory::{
    LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo, ShortTermMemoryIndexPo,
};
use ai_orz::pkg::RequestContext;
use ai_orz::service::domain::runtime::domain as runtime_domain;
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
        "description": "Real embedding provider for memory vector tests",
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

/// 从 MemoryPo 提取 id
fn memory_po_id(po: &MemoryPo) -> String {
    match po {
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        MemoryPo::Trace(t) => t.id.clone(),
        MemoryPo::Relation(r) => r.id.clone(),
    }
}

/// 直接调用 domain layer 创建短期记忆（绕过 HTTP handler 的 agent_id 限制）
async fn create_short_term_memory(
    ctx: &RequestContext,
    agent_id: &str,
    summary: &str,
    task_id: Option<&str>,
    tags: Vec<String>,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let id_content = format!("{}{}", summary, now);
    let id = format!("st_{}", sha256::digest(id_content));
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

    let index = ShortTermMemoryIndexPo {
        id: id.clone(),
        agent_id: agent_id.to_string(),
        task_id: task_id.map(|s| s.to_string()),
        role: "assistant".to_string(),
        summary: summary.to_string(),
        tags: tags_json,
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let results = runtime_domain()
        .memory()
        .create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index))
        .await
        .expect("create short term memory failed");

    results.first().map(|m| memory_po_id(&m.po)).unwrap_or(id)
}

/// 直接调用 domain layer 创建知识节点
async fn create_knowledge_node(
    ctx: &RequestContext,
    agent_id: &str,
    name: &str,
    description: &str,
    summary: &str,
    tags: Vec<String>,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let id_content = format!("{}{}", description, now);
    let id = format!("kn_{}", sha256::digest(id_content));
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    let is_published = tags.iter().any(|t| t == "published");

    let node = LongTermKnowledgeNodePo {
        id: id.clone(),
        agent_id: agent_id.to_string(),
        node_name: name.to_string(),
        node_description: description.to_string(),
        node_type: "general".to_string(),
        summary: summary.to_string(),
        tags: tags_json,
        status: MemoryStatus::Active,
        is_published,
        created_at: now,
        updated_at: now,
    };

    let results = runtime_domain()
        .memory()
        .create(
            ctx.clone(),
            MemoryCreateParams::CreateKnowledgeNode {
                node,
                references: vec![],
            },
        )
        .await
        .expect("create knowledge node failed");

    results.first().map(|m| memory_po_id(&m.po)).unwrap_or(id)
}

/// 通过 HTTP query_memory 查询记忆
async fn query_memory(app: &TestApp, jwt: &str, req: &serde_json::Value) -> Vec<serde_json::Value> {
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/query_memory", req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("results")
        .and_then(|v| v.as_array())
        .expect("missing results")
        .clone()
}

/// 通过 HTTP search_memory 搜索记忆
async fn search_memory(
    app: &TestApp,
    jwt: &str,
    req: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents/search_memory", req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("results")
        .and_then(|v| v.as_array())
        .expect("missing results")
        .clone()
}

// =================================================================
// Part A: CI 默认测试（无 embedding provider，FTS5 路径）
// =================================================================

/// 短期记忆 query_memory 基础查询：创建 → 查询 → 验证返回
#[sqlx::test]
async fn test_memory_query_short_term(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_name = format!("QueryAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    let unique = uuid::Uuid::now_v7().to_string();
    let summary = format!("关于Rust异步编程的讨论-{}", unique);
    create_short_term_memory(&ctx, &agent_id, &summary, None, vec![]).await;

    let results = query_memory(
        &app,
        &jwt,
        &json!({
            "agent_id": agent_id,
            "memory_type": "short_term",
            "limit": 20
        }),
    )
    .await;

    let found = results.iter().any(|m| {
        m.get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.contains(&summary))
            .unwrap_or(false)
    });
    assert!(found, "query_memory should return the created memory");
}

/// task_id 过滤：创建带不同 task_id 的记忆 → 验证过滤生效
#[sqlx::test]
async fn test_memory_query_with_task_id_filter(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        "TaskFilterAgent",
    )
    .await;

    let task_a = format!("task-A-{}", uuid::Uuid::now_v7());
    let task_b = format!("task-B-{}", uuid::Uuid::now_v7());
    let unique = uuid::Uuid::now_v7().to_string();

    create_short_term_memory(
        &ctx,
        &agent_id,
        &format!("任务A的讨论内容-{}", unique),
        Some(&task_a),
        vec![],
    )
    .await;
    create_short_term_memory(
        &ctx,
        &agent_id,
        &format!("任务B的讨论内容-{}", unique),
        Some(&task_b),
        vec![],
    )
    .await;
    create_short_term_memory(
        &ctx,
        &agent_id,
        &format!("无任务关联的记忆-{}", unique),
        None,
        vec![],
    )
    .await;

    // 不带 task_id → 返回全部 3 条
    let all = query_memory(
        &app,
        &jwt,
        &json!({
            "agent_id": agent_id,
            "memory_type": "short_term",
            "limit": 20
        }),
    )
    .await;
    assert_eq!(all.len(), 3, "不带 task_id 应返回全部 3 条");

    // task_id=task_a → 只返回 1 条
    let filtered = query_memory(
        &app,
        &jwt,
        &json!({
            "agent_id": agent_id,
            "memory_type": "short_term",
            "task_id": task_a,
            "limit": 20
        }),
    )
    .await;
    assert_eq!(filtered.len(), 1, "task_id 过滤应只返回 task_a 的 1 条记忆");

    let summary = filtered[0]
        .get("summary")
        .and_then(|v| v.as_str())
        .expect("missing summary");
    assert!(
        summary.contains("任务A"),
        "过滤结果应为任务A的记忆，实际: {}",
        summary
    );
}

/// 短期记忆 FTS5 关键词搜索（无 embedding provider）
#[sqlx::test]
async fn test_memory_search_short_term_fts5(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, "FtsAgent")
            .await;

    let unique = uuid::Uuid::now_v7().to_string();
    create_short_term_memory(
        &ctx,
        &agent_id,
        &format!("关于自然语言处理和文本分析的讨论-{}", unique),
        None,
        vec![],
    )
    .await;
    create_short_term_memory(
        &ctx,
        &agent_id,
        &format!("关于数据库管理和SQL查询优化的讨论-{}", unique),
        None,
        vec![],
    )
    .await;

    let results = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "自然语言",
            "memory_type": "short_term",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;

    let found_nlp = results.iter().any(|m| {
        m.get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("自然语言"))
            .unwrap_or(false)
    });
    assert!(found_nlp, "应通过 FTS5 找到含'自然语言'的记忆");

    let found_sql = results.iter().any(|m| {
        m.get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("SQL"))
            .unwrap_or(false)
    });
    assert!(!found_sql, "搜索'自然语言'时不应返回 SQL 相关的记忆");
}

/// 知识节点 FTS5 搜索（trigram 分词器支持中文）
#[sqlx::test]
async fn test_memory_search_knowledge_node_fts5(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, "KnAgent")
            .await;

    let unique = uuid::Uuid::now_v7().to_string();
    create_knowledge_node(
        &ctx,
        &agent_id,
        &format!("深度学习-{}", unique),
        &format!(
            "深度学习是机器学习的一个分支，使用神经网络进行特征学习-{}",
            unique
        ),
        "深度学习相关知识总结",
        vec![],
    )
    .await;
    create_knowledge_node(
        &ctx,
        &agent_id,
        &format!("前端开发-{}", unique),
        &format!("React 和 Vue 是主流的前端框架-{}", unique),
        "前端开发相关知识",
        vec![],
    )
    .await;

    let results = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "神经网络",
            "memory_type": "knowledge_node",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;

    let found_dl = results.iter().any(|m| {
        m.get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("深度学习"))
            .unwrap_or(false)
    });
    assert!(
        found_dl,
        "应通过 FTS5 找到含'神经网络'的知识节点（深度学习节点描述中包含该词）"
    );

    let found_fe = results.iter().any(|m| {
        m.get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("前端"))
            .unwrap_or(false)
    });
    assert!(!found_fe, "搜索'神经网络'时不应返回前端相关的知识节点");
}

// =================================================================
// Part B: 真实向量搜索测试（ignored，需 API key + 真实 LanceDB）
// =================================================================

/// 真实短期记忆向量语义搜索
///
/// 创建内容含"深度学习"但不出现"神经网络"的记忆
/// → 用"神经网络"搜索（语义相关，未出现在内容中）→ 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_memory_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping memory vector test");
        return;
    };

    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let _embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        "VectorAgent",
    )
    .await;

    let unique = uuid::Uuid::now_v7().to_string();
    let summary = format!("今天讨论了深度学习模型训练与梯度下降优化的方法-{}", unique);
    let memory_id = create_short_term_memory(&ctx, &agent_id, &summary, None, vec![]).await;

    // 等待向量索引完成
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let results = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "神经网络",
            "memory_type": "short_term",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;

    let found = results.iter().any(|m| {
        m.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == memory_id)
            .unwrap_or(false)
    });
    assert!(
        found,
        "memory should be found via semantic vector search for '神经网络' \
         (content mentions '深度学习'); results: {:?}",
        results
            .iter()
            .map(|m| m.get("summary").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Memory vector semantic search test passed");
}

/// 真实向量索引维护：创建 → 搜索验证 → 删除 → 搜索验证已删除
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_memory_vector_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping memory maintenance test");
        return;
    };

    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let _embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, "MaintAgent")
            .await;

    let unique = uuid::Uuid::now_v7().to_string();
    let summary = format!("关于机器学习算法优化和模型部署的实践-{}", unique);
    let memory_id = create_short_term_memory(&ctx, &agent_id, &summary, None, vec![]).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 1. 创建后应可搜索到
    let results = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "人工智能",
            "memory_type": "short_term",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;
    let found_before = results.iter().any(|m| {
        m.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == memory_id)
            .unwrap_or(false)
    });
    assert!(
        found_before,
        "memory should be found via vector search before deletion"
    );

    // 2. 删除记忆（通过 delete_memory handler）
    let (status, _body) = app
        .delete_with_jwt(&format!("/api/v1/hr/agents/memories/{}", memory_id), &jwt)
        .await;
    assert!(
        status.is_success(),
        "delete memory should succeed, got status: {}",
        status
    );

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 3. 删除后应搜索不到
    let results_after = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "人工智能",
            "memory_type": "short_term",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;
    let found_after = results_after.iter().any(|m| {
        m.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == memory_id)
            .unwrap_or(false)
    });
    assert!(
        !found_after,
        "memory should NOT be found via vector search after deletion"
    );

    eprintln!("Memory vector index maintenance test passed");
}

/// 真实混合搜索排序：创建两条记忆，一条同时命中关键词+向量，一条仅向量命中
/// → 验证 Hybrid 排序优先于 Vector
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_memory_hybrid_ranking(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping hybrid ranking test");
        return;
    };

    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let _embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        "HybridAgent",
    )
    .await;

    let unique = uuid::Uuid::now_v7().to_string();

    // 记忆 A：同时包含关键词"机器学习"和语义相关"神经网络"
    let summary_a = format!("机器学习与神经网络的对比分析-{}", unique);
    let mem_a = create_short_term_memory(&ctx, &agent_id, &summary_a, None, vec![]).await;

    // 记忆 B：仅语义相关（含"深度学习"不含"机器学习"）
    let summary_b = format!("深度学习模型训练方法总结-{}", unique);
    let mem_b = create_short_term_memory(&ctx, &agent_id, &summary_b, None, vec![]).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 用"机器学习"搜索（同时 FTS5 + 向量命中记忆 A，仅向量命中记忆 B）
    let results = search_memory(
        &app,
        &jwt,
        &json!({
            "query": "机器学习",
            "memory_type": "short_term",
            "max_results": 20,
            "agent_id": agent_id
        }),
    )
    .await;

    let result_a = results
        .iter()
        .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(mem_a.as_str()));
    let result_b = results
        .iter()
        .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(mem_b.as_str()));

    assert!(result_a.is_some(), "memory A should be found");
    assert!(
        result_b.is_some(),
        "memory B should be found via semantic search"
    );

    // 验证 match_type：A 应为 hybrid，B 应为 vector
    if let Some(a) = result_a {
        let match_type = a
            .get("search_match")
            .and_then(|v| v.get("match_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        assert_eq!(
            match_type, "hybrid",
            "memory A should be hybrid match (FTS5 + Vector)"
        );
    }

    if let Some(b) = result_b {
        let match_type = b
            .get("search_match")
            .and_then(|v| v.get("match_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        assert_eq!(
            match_type, "vector",
            "memory B should be vector-only match (semantic, no FTS5)"
        );
    }

    // 验证排序：A (hybrid) 应排在 B (vector) 之前
    let pos_a = results
        .iter()
        .position(|m| m.get("id").and_then(|v| v.as_str()) == Some(mem_a.as_str()));
    let pos_b = results
        .iter()
        .position(|m| m.get("id").and_then(|v| v.as_str()) == Some(mem_b.as_str()));
    if let (Some(pa), Some(pb)) = (pos_a, pos_b) {
        assert!(
            pa < pb,
            "hybrid match (A) should rank before vector-only match (B)"
        );
    }

    eprintln!("Memory hybrid ranking test passed");
}
