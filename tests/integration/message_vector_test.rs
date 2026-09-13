//! Message 向量搜索集成测试
//!
//! 覆盖：
//! - Message 关键词搜索（FTS5，无 embedding provider 也可工作）
//! - 真实 Embedding provider 场景下的向量语义搜索（ignored，需 API key）
//! - 真实向量索引维护：创建 → 删除（ignored）
//! - 真实混合搜索排序：FTS5 命中 + 向量命中（ignored）
//!
//! Message 的特殊性：
//! - 消息通过 POST /messages/agents 发送（不可编辑，只能删除）
//! - 向量索引在发送时自动 upsert（DAL 层 create 路径）
//! - 搜索通过 POST /messages/search 调用 hybrid search

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ::common::enums::MessageStatus;
use ai_orz::models::vector::{VectorIndexParams, VectorPayload};
use ai_orz::service::dao::message::{MessageQuery, new_message_vector_dao};
use serde_json::json;
use sqlx::SqlitePool;

/// 真实向量用例串行锁：这些用例围绕「全局唯一启用的 embedding provider」
/// 维护建索引窗口（建 provider → 发消息 → 搜索/排序 → 删 provider）。
/// 并发执行时互相拆掉对方赖以检索的 provider 会造成召回落空，故整体串行化。
static REAL_VECTOR_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        "description": "Real embedding provider for message vector tests",
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

/// 创建 Agent（消息接收方）
async fn create_agent(app: &TestApp, jwt: &str, provider_id: &str, name: &str) -> String {
    crate::common::factories::create_test_agent(app, jwt, provider_id, name).await
}

/// 发送消息给 Agent，返回 message_id
async fn send_message(app: &TestApp, jwt: &str, to_agent_id: &str, content: &str) -> String {
    let req = json!({
        "to_agent_id": to_agent_id,
        "content": content,
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string()
}

// =================================================================
// 默认运行的测试（无 embedding provider，FTS5 路径）
// =================================================================

/// Message FTS5 关键词搜索（无 embedding provider，仅走 FTS5 路径）
#[sqlx::test]
async fn test_message_fts5_search(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_name = format!("FtsAgent-{}", uuid::Uuid::now_v7());
    let agent_id = create_agent(&app, &jwt, &bs.chat_provider_id, &agent_name).await;

    let unique = uuid::Uuid::now_v7().to_string();
    let content_a = format!("关于自然语言处理和文本分析的讨论-{}", unique);
    let content_b = format!("关于数据库管理和SQL查询优化的讨论-{}", unique);

    let _msg_a = send_message(&app, &jwt, &agent_id, &content_a).await;
    let _msg_b = send_message(&app, &jwt, &agent_id, &content_b).await;

    // Search by keyword contained in message A's content
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "自然语言", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    let found_a = messages.iter().any(|m| {
        m.get("content")
            .and_then(|v| v.as_str())
            .map(|c| c.contains(&content_a))
            .unwrap_or(false)
    });
    assert!(found_a, "message A should be found via FTS5 keyword match");

    let found_b = messages.iter().any(|m| {
        m.get("content")
            .and_then(|v| v.as_str())
            .map(|c| c.contains(&content_b))
            .unwrap_or(false)
    });
    assert!(
        !found_b,
        "message B should NOT be found when searching for '自然语言'"
    );
}

/// Message 搜索带业务过滤（to_id 过滤）
#[sqlx::test]
async fn test_message_search_with_filter(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_a = create_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FilterAgentA-{}", uuid::Uuid::now_v7()),
    )
    .await;
    let agent_b = create_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FilterAgentB-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let unique = uuid::Uuid::now_v7().to_string();
    let content = format!("关于自然语言处理的讨论-{}", unique);

    // 发送给 agent_a
    let _msg_a = send_message(&app, &jwt, &agent_a, &content).await;
    // 发送给 agent_b
    let _msg_b = send_message(&app, &jwt, &agent_b, &content).await;

    // 搜索关键词 + 过滤 to_id = agent_a
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "自然语言", "to_id": agent_a, "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    // 只应返回发给 agent_a 的消息
    for m in messages {
        let to_id = m.get("to_id").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(
            to_id, agent_a,
            "filter by to_id should only return messages to agent_a"
        );
    }
}

// =================================================================
// 真实向量搜索测试（ignored，需 API key + 真实 LanceDB）
// =================================================================

/// 真实 Message 向量语义搜索：发送内容含"深度学习"但不出现"神经网络"的消息
/// → 用语义相关但不出现的关键词"神经网络"搜索 → 验证可召回
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_message_vector_search(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping message vector test");
        return;
    };
    // 串行锁保护建索引窗口（静态声明处的文档注释说明原因）
    let _guard = REAL_VECTOR_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建 embedding provider
    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    // 2. 创建 Agent
    let agent_name = format!("VectorAgent-{}", uuid::Uuid::now_v7());
    let agent_id = create_agent(&app, &jwt, &bs.chat_provider_id, &agent_name).await;

    // 3. 发送消息，内容含"深度学习"但不出现"神经网络"
    let unique = uuid::Uuid::now_v7().to_string();
    let content = format!(
        "今天我们讨论一下深度学习模型训练与梯度下降优化的方法-{}",
        unique
    );
    let message_id = send_message(&app, &jwt, &agent_id, &content).await;

    // 等待向量索引完成
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 4. 用"神经网络"搜索（语义相关，未出现在描述中）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "神经网络", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    let found = messages
        .iter()
        .any(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(message_id.as_str()));
    assert!(
        found,
        "message should be found via semantic vector search for '神经网络' \
         (content mentions '深度学习'); messages: {:?}",
        messages
            .iter()
            .map(|m| m.get("content").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    eprintln!("Message vector semantic search test passed");

    // 清理
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// 真实 Message 向量索引创建验证：发送 → 等待索引 → 语义搜索验证
///
/// Message 不支持 HTTP DELETE，因此无法测试索引删除路径。
/// 本测试改为验证向量索引创建：发送内容含"机器学习"但不出现"人工智能"的消息
/// → 用语义相关但不出现的关键词"人工智能"搜索 → 验证可召回且 match_type 包含 vector
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_message_vector_maintenance(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping message maintenance test");
        return;
    };
    // 串行锁保护建索引窗口（静态声明处的文档注释说明原因）
    let _guard = REAL_VECTOR_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_name = format!("MaintAgent-{}", uuid::Uuid::now_v7());
    let agent_id = create_agent(&app, &jwt, &bs.chat_provider_id, &agent_name).await;

    // 发送消息，内容含"机器学习"但不出现"人工智能"
    let unique = uuid::Uuid::now_v7().to_string();
    let content = format!("关于机器学习算法优化和模型部署的实践-{}", unique);
    let message_id = send_message(&app, &jwt, &agent_id, &content).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 用"人工智能"搜索（语义相关，未出现在内容中）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "人工智能", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    let target = messages
        .iter()
        .find(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(message_id.as_str()));
    assert!(
        target.is_some(),
        "message should be found via semantic vector search for '人工智能' \
         (content mentions '机器学习'); messages: {:?}",
        messages
            .iter()
            .map(|m| m.get("content").and_then(|v| v.as_str()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    // 验证 match_type 包含 vector（证明向量索引已创建且被使用）
    let match_type = target
        .and_then(|m| m.get("match_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        match_type == "vector" || match_type == "hybrid",
        "message found via semantic search should have match_type 'vector' or 'hybrid', got '{}'",
        match_type
    );

    eprintln!(
        "Message vector index creation test passed: match_type={}",
        match_type
    );

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// 真实混合搜索排序：同时发送 FTS5 匹配和向量匹配的消息
/// 验证 FTS5 命中排名高于向量命中（与其他实体一致）
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_message_hybrid_ranking(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping hybrid ranking test");
        return;
    };
    // 串行锁保护建索引窗口（静态声明处的文档注释说明原因）
    let _guard = REAL_VECTOR_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_name = format!("HybridAgent-{}", uuid::Uuid::now_v7());
    let agent_id = create_agent(&app, &jwt, &bs.chat_provider_id, &agent_name).await;

    let unique = uuid::Uuid::now_v7().to_string();

    // Message A: 内容包含关键词"自然语言处理"（FTS5 命中）
    let content_a = format!("专注于自然语言处理和文本分析的讨论-{}", unique);
    let msg_a_id = send_message(&app, &jwt, &agent_id, &content_a).await;

    // Message B: 内容语义相关但不包含关键词（向量命中）
    let content_b = format!("关于语义理解和文本挖掘的技术交流-{}", unique);
    let msg_b_id = send_message(&app, &jwt, &agent_id, &content_b).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 搜索"自然语言处理"：Message A 应 FTS5 命中，Message B 应向量命中
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "自然语言处理", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    let found_a = messages
        .iter()
        .any(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(msg_a_id.as_str()));
    let found_b = messages
        .iter()
        .any(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(msg_b_id.as_str()));
    assert!(found_a, "Message A should be found via FTS5");
    assert!(found_b, "Message B should be found via vector");

    // FTS5 命中应排名更高
    if let (Some(pos_a), Some(pos_b)) = (
        messages
            .iter()
            .position(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(msg_a_id.as_str())),
        messages
            .iter()
            .position(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(msg_b_id.as_str())),
    ) {
        assert!(
            pos_a < pos_b,
            "FTS5 match (Message A, pos={}) should rank higher than vector match (Message B, pos={})",
            pos_a,
            pos_b
        );
    }

    eprintln!("Message hybrid search ranking test passed");

    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", embedding_provider_id),
            &jwt,
        )
        .await;
}

/// 验证 match_type 字段正确标记（hybrid/vector/keyword）
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_message_match_type(pool: SqlitePool) {
    let Some(cfg) = RealModelConfig::from_env() else {
        eprintln!("SKIP: TEST_EMBEDDING_API_KEY not set, skipping match_type test");
        return;
    };
    // 串行锁保护建索引窗口（静态声明处的文档注释说明原因）
    let _guard = REAL_VECTOR_MUTEX.lock().await;

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let embedding_provider_id = create_embedding_provider(&app, &jwt, &cfg).await;

    let agent_name = format!("MatchTypeAgent-{}", uuid::Uuid::now_v7());
    let agent_id = create_agent(&app, &jwt, &bs.chat_provider_id, &agent_name).await;

    let unique = uuid::Uuid::now_v7().to_string();

    // 发送包含"自然语言处理"的消息
    let content = format!("专注于自然语言处理和文本分析的讨论-{}", unique);
    let msg_id = send_message(&app, &jwt, &agent_id, &content).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 搜索"自然语言处理"：应该同时命中 FTS5 + 向量（Hybrid）
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/messages/search",
            &json!({"keyword": "自然语言处理", "limit": 20}),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages");

    let target = messages
        .iter()
        .find(|m| m.get("message_id").and_then(|v| v.as_str()) == Some(msg_id.as_str()))
        .expect("message should be found");

    let match_type = target
        .get("match_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        match_type == "hybrid" || match_type == "keyword",
        "message with FTS5 match should have match_type 'hybrid' or 'keyword', got '{}'",
        match_type
    );

    // 验证 vector_distance 和 fts_rank 至少有一个有值
    let has_vector_distance = target
        .get("vector_distance")
        .and_then(|v| v.as_f64())
        .is_some();
    let has_fts_rank = target.get("fts_rank").and_then(|v| v.as_f64()).is_some();
    assert!(
        has_vector_distance || has_fts_rank,
        "hybrid match should have at least one of vector_distance or fts_rank"
    );

    eprintln!(
        "Message match_type test passed: match_type={}, vector_distance={:?}, fts_rank={:?}",
        match_type,
        target.get("vector_distance").and_then(|v| v.as_f64()),
        target.get("fts_rank").and_then(|v| v.as_f64())
    );

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
fn manual_vector_params(vector: Vec<f32>, payload: VectorPayload) -> VectorIndexParams {
    let payload_hash = payload.hash();
    VectorIndexParams {
        vector,
        content_hash: sha256::digest(uuid::Uuid::now_v7().to_string()),
        payload,
        payload_hash,
        model_provider_id: "manual-test-provider".to_string(),
        embedding_model: "manual-test-model".to_string(),
        expire_at: None,
    }
}

/// pre-filter 谓词下推：组织隔离 + 接收方隔离，Top-K 不被跨租户数据稀释
///
/// 数据布局（query=[1,0]，余弦距离）：
/// - org-2 发给 agent-2 的消息全局最近（distance=0）：若为 post-filter（先取 Top-K 再过滤），
///   org-1 视角 top_k=1 会取到它再被过滤掉 → 返回空；
///   正确的 pre-filter 应在满足谓词的候选集内选取 → 命中 org-1 自己的消息。
#[sqlx::test]
async fn test_message_vector_search_prefilter_isolation(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    // 唯一化 ID：同一 binary 内兄弟用例共享进程级 vector store，防串扰
    let org_1 = format!("pf-org-1-{}", uuid::Uuid::now_v7());
    let org_2 = format!("pf-org-2-{}", uuid::Uuid::now_v7());
    let agent_1 = format!("pf-agent-1-{}", uuid::Uuid::now_v7());
    let agent_2 = format!("pf-agent-2-{}", uuid::Uuid::now_v7());

    let query_vector: Vec<f32> = vec![1.0, 0.0];
    let other_org_msg_id = format!("msg_pf_{}", uuid::Uuid::now_v7());
    let own_msg_id = format!("msg_pf_{}", uuid::Uuid::now_v7());

    let store = ctx.vector_store();
    store
        .upsert(
            "messages",
            &other_org_msg_id,
            &manual_vector_params(
                vec![1.0, 0.0],
                VectorPayload {
                    org_id: Some(org_2.clone()),
                    to_id: Some(agent_2.clone()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert other-org message failed");
    store
        .upsert(
            "messages",
            &own_msg_id,
            &manual_vector_params(
                vec![0.6, 0.8],
                VectorPayload {
                    org_id: Some(org_1.clone()),
                    to_id: Some(agent_1.clone()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert own-org message failed");

    let dao = new_message_vector_dao();

    // 基线：无 filter 时全局最近的是 org-2 的消息（证明数据已落库且距离排序生效）
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &MessageQuery::default())
        .await
        .expect("search without filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].row.id, other_org_msg_id,
        "无 filter 时全局最近应为 org-2 的消息"
    );

    // 组织隔离（多租户）：org-1 视角 top_k=1 必须命中自己的消息（post-filter 实现会返回空）
    let org_query = MessageQuery {
        organization_id: Some(org_1.clone()),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &org_query)
        .await
        .expect("search with org filter failed");
    assert_eq!(
        hits.len(),
        1,
        "pre-filter 后 top-1 必须命中（post-filter 实现会返回空）"
    );
    assert_eq!(hits[0].row.id, own_msg_id);
    assert_eq!(hits[0].row.payload.org_id.as_deref(), Some(org_1.as_str()));

    // 接收方隔离：to_id=agent-1 视角 top_k=1 同样必须命中自己的消息
    let to_query = MessageQuery {
        to_id: Some(agent_1.clone()),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &to_query)
        .await
        .expect("search with to_id filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, own_msg_id);
    assert_eq!(hits[0].row.payload.to_id.as_deref(), Some(agent_1.as_str()));

    // 放大 top_k：全部命中均属于 org-1 / agent-1
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 5, &org_query)
        .await
        .expect("search with org filter (top 5) failed");
    assert!(!hits.is_empty());
    for hit in &hits {
        assert_eq!(
            hit.row.payload.org_id.as_deref(),
            Some(org_1.as_str()),
            "组织隔离：全部命中均应属于 org-1"
        );
    }
    assert!(
        !hits.iter().any(|h| h.row.id == other_org_msg_id),
        "组织隔离：org-2 的消息不应出现"
    );
}

/// pre-filter 状态可见性：status_in 下推排除已撤回（Recalled）的消息
///
/// 数据布局（query=[0,1]，余弦距离；方向与 isolation 用例正交，避免共享
/// vector store 中的兄弟用例数据干扰基线断言）：
/// - Recalled 消息全局最近（distance=0），但 status_in=[Pending, Processed] 视角不可见
/// - Processed 消息稍远（≈0.04），status_in 视角可见
#[sqlx::test]
async fn test_message_vector_search_prefilter_status_in(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    let query_vector: Vec<f32> = vec![0.0, 1.0];
    let recalled_msg_id = format!("msg_sv_{}", uuid::Uuid::now_v7());
    let processed_msg_id = format!("msg_sv_{}", uuid::Uuid::now_v7());

    let store = ctx.vector_store();
    store
        .upsert(
            "messages",
            &recalled_msg_id,
            &manual_vector_params(
                vec![0.0, 1.0],
                VectorPayload {
                    status: Some(MessageStatus::Recalled.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert recalled message failed");
    store
        .upsert(
            "messages",
            &processed_msg_id,
            &manual_vector_params(
                vec![0.28, 0.96],
                VectorPayload {
                    status: Some(MessageStatus::Processed.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert processed message failed");

    let dao = new_message_vector_dao();

    // 基线：无 filter 时全局最近的是 Recalled 消息
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &MessageQuery::default())
        .await
        .expect("search without filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, recalled_msg_id);

    // status_in 下推：top_k=1 必须命中可见的 Processed 消息（而非全局最近但已撤回的）
    let status_query = MessageQuery {
        status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processed]),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &status_query)
        .await
        .expect("search with status_in filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].row.id, processed_msg_id,
        "top-1 应命中最近的可见行（Processed），而非全局最近但已撤回的 Recalled"
    );

    // 放大 top_k：Recalled 消息不应出现
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 5, &status_query)
        .await
        .expect("search with status_in filter (top 5) failed");
    assert!(
        !hits.iter().any(|h| h.row.id == recalled_msg_id),
        "status_in 过滤：已撤回的消息不应出现"
    );
}
