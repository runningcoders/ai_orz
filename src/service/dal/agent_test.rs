//! Agent DAL 单元测试

use crate::models::agent::{Agent, AgentPo};
use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::MatchType;
use crate::pkg::RequestContext;
use crate::service::dal::agent::{dal, init, new};
use crate::service::dao::agent::init as agent_dao_init;
use crate::service::dao::agent::{self, AgentSearch};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

/// 初始化测试环境
async fn init_test_env(
    pool: SqlitePool,
) -> (
    Arc<dyn crate::service::dal::agent::AgentDal + Send + Sync>,
    RequestContext,
) {
    agent_dao_init();
    crate::service::dao::cortex::init();
    crate::service::dao::model_provider::init();
    init();
    let dal = dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试 Agent
fn create_test_agent(name: &str, provider_id: &str) -> Agent {
    let agent_po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "".to_string(),
        vec![],
        "".to_string(),
        provider_id.to_string(),
        "admin".to_string(),
    );
    Agent::from_po(agent_po)
}

// ==================== Mock 实现（用于搜索测试） ====================

/// Mock CortexDao，返回 mock 向量（不依赖真实的 LLM）
#[derive(Clone, Debug)]
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "Mock response".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let mut result = Vec::with_capacity(texts.len());
        for text in texts {
            let vec = if text.contains("nonexistent") {
                vec![1.0, 0.0, 0.0]
            } else {
                vec![0.0, 1.0, 1.0]
            };
            result.push(vec);
        }
        Ok(result)
    }
}

/// Mock ModelProviderDao，返回测试用的 ModelProvider
#[derive(Clone, Debug)]
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: vec![mock_provider()],
            total: 1,
        })
    }

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(vec![])
    }

    async fn update(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }

    async fn delete(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }

    async fn get_default_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(Some(mock_provider()))
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo {
        id: "mock-provider".to_string(),
        name: "Mock Provider".to_string(),
        provider_type: common::enums::ProviderType::Ollama,
        model_name: "mock-embedding".to_string(),
        capability: common::enums::ModelCapability::Embedding,
        api_key: "".to_string(),
        base_url: Some("http://localhost:11434".to_string()),
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }
}

/// 创建测试 RequestContext
fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化搜索测试环境（使用 Mock CortexDao 和 ModelProviderDao）
async fn init_search_test_env(
    _pool: SqlitePool,
) -> Arc<dyn crate::service::dal::agent::AgentDal + Send + Sync> {
    // 初始化基础 DAO（agent + vector）
    agent::init();
    // 初始化 stats DAO（DAL new() 需要传入）
    agent::stats_init();
    crate::service::dao::tool::stats_init();
    crate::service::dao::model_provider::stats_init();

    // 创建自定义 DAL，使用 Mock 实现
    new(
        agent::dao(),
        agent::vector_dao(),
        agent::stats_dao(),
        crate::service::dao::tool::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        Arc::new(MockCortexDao),
        Arc::new(MockModelProviderDao),
    )
}

/// 创建带描述和能力列表的测试 Agent
fn create_test_agent_full(
    name: &str,
    description: &str,
    capabilities: Vec<&str>,
    provider_id: &str,
) -> Agent {
    let agent_po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        description.to_string(),
        capabilities.into_iter().map(String::from).collect(),
        "".to_string(),
        provider_id.to_string(),
        "admin".to_string(),
    );
    Agent::from_po(agent_po)
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent = create_test_agent("TestAgent", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    let found: Option<Agent> = dal.find_by_id(ctx, agent.id()).await.unwrap();
    assert_eq!(found.as_ref().unwrap().name(), "TestAgent");
    assert_eq!(found.unwrap().po.created_by, "admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..3 {
        let agent = create_test_agent(&format!("Agent{}", i), &format!("provider-{}", i));
        dal.create(ctx.clone(), &agent).await.unwrap();
    }

    let all: Vec<Agent> = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool.clone()).await;

    let agent = create_test_agent("Original", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    dal.update(
        crate::pkg::request_context_test_support::new_test_ctx("editor", pool),
        &updated,
    )
    .await
    .unwrap();

    let found: Option<Agent> = dal.find_by_id(ctx, updated.id()).await.unwrap();
    assert_eq!(found.as_ref().unwrap().name(), "Updated");
    assert_eq!(found.unwrap().po.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent = create_test_agent("ToDelete", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    dal.delete(ctx.clone(), &agent).await.unwrap();
    let found: Option<Agent> = dal.find_by_id(ctx, agent.id()).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_find_all_excludes_deleted(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent1 = create_test_agent("Normal", "provider-id-1");
    let agent2 = create_test_agent("Deleted", "provider-id-2");

    dal.create(ctx.clone(), &agent1).await.unwrap();
    dal.create(ctx.clone(), &agent2).await.unwrap();
    dal.delete(ctx.clone(), &agent2).await.unwrap();

    let all: Vec<Agent> = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 1);
    let names: Vec<String> = all.iter().map(|a| a.name().to_string()).collect();
    assert!(names.contains(&"Normal".to_string()));
    assert!(!names.contains(&"Deleted".to_string()));
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let found: Option<Agent> = dal.find_by_id(ctx, "not-exists").await.unwrap();
    assert!(found.is_none());
}

// ==================== FTS5 + 向量搜索 DAL 层测试 ====================

/// 测试 FTS5 基本搜索（按 name 匹配）
#[sqlx::test]
async fn test_search_fts5_by_name(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "debug-helper",
        "Helps with debugging",
        vec!["debug"],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 搜索：按名称匹配
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].name(), "debug-helper");

    Ok(())
}

/// 测试 FTS5 搜索（按 description 匹配）
#[sqlx::test]
async fn test_search_fts5_by_description(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "python-tool",
        "Helps with debugging code",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 搜索：按描述匹配（名称不含 "debugging"）
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debugging".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].name(), "python-tool");

    Ok(())
}

/// 测试 FTS5 中文搜索（trigram 分词器需要 3+ 字符）
#[sqlx::test]
async fn test_search_fts5_chinese(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建中文 Agent（3+ 中文字符才能被 trigram 匹配）
    let agent = create_test_agent_full(
        "测试助手",
        "这是一个用于测试的智能代理",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 3 字符搜索 → 能匹配
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("测试助".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 1, "3 字符中文搜索应能匹配");

    // 3 字符搜索描述中的内容
    let results2 = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("智能代".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results2.items.len(), 1, "3 字符中文搜索描述应能匹配");

    Ok(())
}

/// 测试 FTS5 无匹配结果
#[sqlx::test]
async fn test_search_fts5_no_match(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent =
        create_test_agent_full("debug-helper", "Helps with debugging", vec![], "provider-1");
    dal.create(ctx.clone(), &agent).await?;

    // 搜索：无匹配
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("nonexistent-keyword-xyz".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 0);

    Ok(())
}

/// 测试三态匹配：Hybrid（FTS5 命中 + 向量命中）
///
/// MockCortexDao 向量策略：
/// - 文本含 "nonexistent" → [1.0, 0.0, 0.0]
/// - 其他文本 → [0.0, 1.0, 1.0]
///
/// 场景：
/// - agent "debug-helper"：FTS5 命中 "debug" + 向量 [0.0, 1.0, 1.0]
/// - 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]，距离 0.0 < 0.8 → Hybrid
#[sqlx::test]
async fn test_search_hybrid_match(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "debug-helper",
        "Helps with debugging",
        vec!["debug"],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1);

    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    assert_eq!(match_info.match_type, MatchType::Hybrid, "应是 Hybrid 匹配");
    assert!(
        match_info.vector_distance.is_some(),
        "vector_distance 应有值"
    );
    assert!(match_info.fts_rank.is_some(), "fts_rank 应有值");

    Ok(())
}

/// 测试三态匹配：Vector-only（FTS5 未命中，仅向量命中）
///
/// 场景：
/// - agent "python-tool"：FTS5 不命中 "debug" + 向量 [0.0, 1.0, 1.0]
/// - 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]，距离 0.0 < 0.8 → Vector
#[sqlx::test]
async fn test_search_vector_only_match(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建名称和描述都不含 "debug" 的 Agent
    let agent = create_test_agent_full(
        "python-tool",
        "A python utility tool",
        vec!["python"],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1, "应返回 1 条 Vector 匹配结果");
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    assert_eq!(match_info.match_type, MatchType::Vector, "应是 Vector 匹配");
    assert!(
        match_info.vector_distance.is_some(),
        "vector_distance 应有值"
    );
    assert!(match_info.fts_rank.is_none(), "fts_rank 应为 None");

    Ok(())
}

/// 测试三态匹配：Keyword-only（FTS5 命中，向量距离 > 阈值）
///
/// 场景：
/// - agent "nonexistent-debug-tool"：FTS5 命中 "debug" + 向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// - 搜索 "debug"：查询向量 [0.0, 1.0, 1.0]，距离 1.0 > 0.8 → Keyword
#[sqlx::test]
async fn test_search_keyword_only_match(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建 name 含 "nonexistent" 的 Agent（向量会是 [1.0, 0.0, 0.0]）
    let agent = create_test_agent_full(
        "nonexistent-debug-tool",
        "A tool for nonexistent debugging",
        vec!["debug"],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1, "应返回 1 条 Keyword 匹配结果");
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    assert_eq!(
        match_info.match_type,
        MatchType::Keyword,
        "应是 Keyword 匹配"
    );
    assert!(match_info.fts_rank.is_some(), "fts_rank 应有值");
    assert!(
        match_info.vector_distance.is_none(),
        "vector_distance 应为 None"
    );

    Ok(())
}

/// 测试综合排序：Hybrid → Vector → Keyword
#[sqlx::test]
async fn test_search_comprehensive_sorting(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 1. Hybrid：name 含 "debug"，向量 [0.0, 1.0, 1.0]
    let hybrid_agent = create_test_agent_full(
        "debug-hybrid",
        "Debug skill hybrid",
        vec!["debug"],
        "provider-1",
    );
    dal.create(ctx.clone(), &hybrid_agent).await?;

    // 2. Vector-only：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
    let vector_agent = create_test_agent_full(
        "vector-only-tool",
        "A vector only tool",
        vec!["utility"],
        "provider-1",
    );
    dal.create(ctx.clone(), &vector_agent).await?;

    // 3. Keyword-only：name 含 "debug" + "nonexistent"，向量 [1.0, 0.0, 0.0]
    let keyword_agent = create_test_agent_full(
        "nonexistent-debug-keyword",
        "Keyword only debug",
        vec!["debug"],
        "provider-1",
    );
    dal.create(ctx.clone(), &keyword_agent).await?;

    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 3, "应返回 3 条结果");

    // 验证排序：Hybrid → Vector → Keyword
    assert_eq!(results.items[0].name(), "debug-hybrid");
    assert_eq!(
        results.items[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid
    );

    assert_eq!(results.items[1].name(), "vector-only-tool");
    assert_eq!(
        results.items[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector
    );

    assert_eq!(results.items[2].name(), "nonexistent-debug-keyword");
    assert_eq!(
        results.items[2].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword
    );

    Ok(())
}

/// 测试向量索引自动维护：create 后自动写入向量
#[sqlx::test]
async fn test_vector_auto_maintenance_create(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "auto-vector-test",
        "Test auto vectorization",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 通过向量搜索验证向量已写入
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("auto".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // 应能通过向量搜索找到（Vector 或 Hybrid 匹配）
    assert_eq!(results.items.len(), 1);
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    assert!(
        match_info.match_type == MatchType::Hybrid || match_info.match_type == MatchType::Vector,
        "create 后应自动写入向量，匹配类型应为 Hybrid 或 Vector"
    );
    assert!(
        match_info.vector_distance.is_some(),
        "vector_distance 应有值"
    );

    Ok(())
}

/// 测试向量索引自动维护：update 后自动更新向量（内容哈希检查）
#[sqlx::test]
async fn test_vector_auto_maintenance_update(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建 Agent
    let mut agent = create_test_agent_full(
        "original-name",
        "Original description",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 更新 Agent（改变内容）
    agent.po.name = "updated-debug-name".to_string();
    agent.po.description = "Updated description with debug".to_string();
    dal.update(ctx.clone(), &agent).await?;

    // 搜索更新后的内容
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].name(), "updated-debug-name");
    // 更新后向量应已重新生成（内容变化触发重索引）
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    assert!(
        match_info.vector_distance.is_some(),
        "update 后向量应已重新生成"
    );

    Ok(())
}

/// 测试向量索引自动维护：delete 后自动删除向量
#[sqlx::test]
async fn test_vector_auto_maintenance_delete(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "delete-vector-test",
        "Test delete vectorization",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 删除 Agent
    dal.delete(ctx.clone(), &agent).await?;

    // 删除后搜索不应找到
    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("delete".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.items.len(), 0, "删除后搜索不应返回结果");

    Ok(())
}

/// 测试 AgentQuery.keyword 兼容性：query 方法中 keyword 已废弃，仅记录 warn 不影响查询
#[sqlx::test]
async fn test_query_keyword_deprecated(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建 Agent
    let agent = create_test_agent_full(
        "compat-test-agent",
        "Test compatibility",
        vec![],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    // 使用 query 方法（keyword 字段已废弃，应被忽略）
    use crate::service::dao::agent::AgentQuery;
    let results = dal
        .query(
            ctx.clone(),
            AgentQuery {
                keyword: Some("compat".to_string()),
                exclude_status: Some(common::enums::AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;

    // keyword 被忽略，但应返回所有 Agent（因为没设其他过滤条件）
    assert_eq!(
        results.items.len(),
        1,
        "query 方法 keyword 被忽略，应返回全部结果"
    );
    assert_eq!(results.items[0].name(), "compat-test-agent");

    Ok(())
}

/// 测试 AgentQuery.ids 批量查询
#[sqlx::test]
async fn test_query_by_ids(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建 3 个 Agent
    let mut agent_ids = Vec::new();
    for i in 0..3 {
        let agent = create_test_agent_full(
            &format!("agent-{}", i),
            &format!("Description {}", i),
            vec![],
            "provider-1",
        );
        dal.create(ctx.clone(), &agent).await?;
        agent_ids.push(agent.id().to_string());
    }

    // 批量查询前 2 个
    use crate::service::dao::agent::AgentQuery;
    let results = dal
        .query(
            ctx.clone(),
            AgentQuery {
                ids: Some(agent_ids[0..2].to_vec()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 2, "应返回 2 条结果（按 IDs 批量查询）");

    Ok(())
}

/// 测试 fts_rank 透传（从 DAO 到 Agent 实体）
#[sqlx::test]
async fn test_search_fts_rank_transparency(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let agent = create_test_agent_full(
        "rust-programming",
        "A rust programming agent",
        vec!["rust"],
        "provider-1",
    );
    dal.create(ctx.clone(), &agent).await?;

    let results = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("rust".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(results.items.len(), 1);
    let match_info = results.items[0]
        .search_match
        .as_ref()
        .expect("search_match 不应为 None");
    // Hybrid 匹配时 fts_rank 应有值
    assert!(
        match_info.fts_rank.is_some(),
        "fts_rank 应有值（从 DAO 透传）"
    );

    Ok(())
}

/// 测试 search 方法的 runtime_state 内存过滤
///
/// runtime_state 是内存态（AgentRuntimeStateManager），DAO 层无法 SQL 过滤。
/// DAL 层 search 方法应在聚合结果后注入 runtime_info，再按 runtime_state 过滤。
#[sqlx::test]
async fn test_search_with_runtime_state_filter(pool: SqlitePool) -> Result<()> {
    let dal = init_search_test_env(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);
    let manager = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global();

    // 创建两个 Agent（name 都含 "debug" 以保证 FTS5 命中）
    let agent_idle = create_test_agent_full("debug-idle", "Idle debug agent", vec![], "p1");
    let agent_busy = create_test_agent_full("debug-busy", "Busy debug agent", vec![], "p1");
    dal.create(ctx.clone(), &agent_idle).await?;
    dal.create(ctx.clone(), &agent_busy).await?;

    // 设置 runtime_state：idle → Idle, busy → Busy
    manager.set_idle(agent_idle.id());
    manager.set_busy(agent_busy.id(), "msg-test-1");

    // 搜索 Idle Agent
    let result_idle = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                filters: crate::service::dao::agent::AgentQuery {
                    runtime_state: Some(common::enums::AgentRuntimeState::Idle),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;

    // 只应返回 Idle Agent
    assert_eq!(
        result_idle.items.len(),
        1,
        "runtime_state=Idle 应只返回 Idle Agent"
    );
    assert_eq!(result_idle.items[0].name(), "debug-idle");
    assert!(result_idle.items.iter().all(|a| {
        a.runtime_info
            .as_ref()
            .map(|i| i.state)
            .unwrap_or(common::enums::AgentRuntimeState::Idle)
            == common::enums::AgentRuntimeState::Idle
    }));

    // 搜索 Busy Agent
    let result_busy = dal
        .search(
            ctx.clone(),
            AgentSearch {
                keyword: Some("debug".to_string()),
                filters: crate::service::dao::agent::AgentQuery {
                    runtime_state: Some(common::enums::AgentRuntimeState::Busy),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(
        result_busy.items.len(),
        1,
        "runtime_state=Busy 应只返回 Busy Agent"
    );
    assert_eq!(result_busy.items[0].name(), "debug-busy");

    // 不设 runtime_state 过滤 → 返回全部
    let result_all = dal
        .search(
            ctx,
            AgentSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(result_all.items.len(), 2, "无 runtime_state 过滤应返回全部");

    // 清理 runtime_state
    manager.set_idle(agent_busy.id());

    Ok(())
}

// ==================== PromptBuilder 单元测试 ====================

use crate::models::prompt_builder::PromptBuilder;
use crate::service::dal::agent::DefaultPromptBuilder;

#[test]
fn prompt_builder_empty() {
    let builder = DefaultPromptBuilder::new();
    assert!(builder.build().is_empty());
}

#[test]
fn prompt_builder_only_system() {
    use crate::models::agent::AgentPo;

    let agent_po = AgentPo::new(
        "测试助手".to_string(),
        vec!["助手".to_string()],
        "我是一个测试助手".to_string(),
        vec!["测试能力".to_string()],
        "你是一个严谨、专业、乐于助人的助手。总是给出准确、有用的回答。".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    let mut builder = DefaultPromptBuilder::new();
    builder.system_prompt(&agent);
    let prompt = builder.build();

    assert!(prompt.contains("【Agent ID】"));
    assert!(prompt.contains("【Agent 名称】"));
    assert!(prompt.contains("测试助手"));
    assert!(prompt.contains("【角色描述】"));
    assert!(prompt.contains("【灵魂设定】"));
    assert!(prompt.contains("严谨、专业、乐于助人"));
}

/// 验证工具注入时不泄露 config 敏感信息
#[test]
fn prompt_builder_includes_tools_without_server_config_details() {
    use crate::models::agent::AgentPo;
    use crate::models::tool::ToolPo;
    use serde_json::json;

    let agent_po = AgentPo::new(
        "工具助手".to_string(),
        vec!["mcp".to_string()],
        "可以使用工具".to_string(),
        vec!["工具调用".to_string()],
        "按需使用工具。".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    let mcp_tool_po = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "mcp.echo-server.echo".to_string(),
        "Echo input text".to_string(),
        common::enums::ToolProtocol::Mcp,
        json!({
            "server_id": "echo-server",
            "tool_name": "echo",
            "command": "python3 /tmp/private_echo_server.py",
            "env": {"PRIVATE_VALUE": "placeholder-value"},
            "url": "https://internal.example.test/mcp"
        }),
        Some(json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        })),
        vec!["mcp".to_string(), "echo-server".to_string()],
        Some("creator".to_string()),
    );

    let mut builder = DefaultPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.tools(&[mcp_tool_po]);
    let prompt = builder.build();

    // 工具说明应出现在某个区块中（神经工具或常用工具）
    assert!(prompt.contains("【常用工具】"));
    assert!(prompt.contains("mcp.echo-server.echo"));
    assert!(prompt.contains("Echo input text"));
    // 不应泄露敏感配置
    assert!(!prompt.contains("python3"));
    assert!(!prompt.contains("PRIVATE_VALUE"));
    assert!(!prompt.contains("placeholder-value"));
    assert!(!prompt.contains("internal.example.test"));
    assert!(!prompt.contains("server_id"));
    assert!(!prompt.contains("tool_name"));
}

/// 验证通过 trait 注入工具后能出现在 Prompt 中
#[test]
fn prompt_builder_trait_tools_injects_into_prompt() {
    use crate::models::agent::AgentPo;
    use crate::models::tool::ToolPo;
    use serde_json::json;

    let agent_po = AgentPo::new(
        "工具助手".to_string(),
        vec!["neural".to_string()],
        "可以使用工具".to_string(),
        vec!["工具调用".to_string()],
        "按需使用工具。".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    let mut neural_tool_po = ToolPo::new(
        "neural.search_web".to_string(),
        "neural.search_web".to_string(),
        "网页搜索工具".to_string(),
        common::enums::ToolProtocol::Mcp, // Manual control_mode
        json!({}),
        Some(json!({"type": "object"})),
        vec!["neural".to_string()],
        Some("creator".to_string()),
    );
    neural_tool_po.control_mode = common::enums::ControlMode::Manual;

    let mut builder: Box<dyn PromptBuilder> = Box::new(DefaultPromptBuilder::new());
    builder.system_prompt(&agent);
    builder.tools(&[neural_tool_po]);

    let prompt = builder.build();
    assert!(prompt.contains("【神经工具】"));
    assert!(prompt.contains("neural.search_web"));
    assert!(prompt.contains("网页搜索工具"));
}

/// 验证按 tag 分块：neural 工具进入神经工具区块，非 neural 工具进入常用工具区块
#[test]
fn prompt_builder_tag_based_block_split() {
    use crate::models::agent::AgentPo;
    use crate::models::tool::ToolPo;
    use serde_json::json;

    let agent_po = AgentPo::new(
        "HR 助手".to_string(),
        vec!["hr".to_string()],
        "HR 工具".to_string(),
        vec!["HR 能力".to_string()],
        "HR 灵魂".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    let neural_tool_po = ToolPo::new(
        "neural.memory_search".to_string(),
        "neural.memory_search".to_string(),
        "神经记忆搜索".to_string(),
        common::enums::ToolProtocol::Mcp,
        json!({}),
        Some(json!({"type": "object"})),
        vec!["neural".to_string()],
        Some("creator".to_string()),
    );
    let hr_tool_po = ToolPo::new(
        "hr.leave_query".to_string(),
        "hr.leave_query".to_string(),
        "请假查询".to_string(),
        common::enums::ToolProtocol::Mcp,
        json!({}),
        Some(json!({"type": "object"})),
        vec!["hr".to_string()],
        Some("creator".to_string()),
    );
    let unmatched_tool_po = ToolPo::new(
        "finance.budget".to_string(),
        "finance.budget".to_string(),
        "预算查询".to_string(),
        common::enums::ToolProtocol::Mcp,
        json!({}),
        Some(json!({"type": "object"})),
        vec!["finance".to_string()],
        Some("creator".to_string()),
    );

    let mut builder = DefaultPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.tools(&[neural_tool_po, hr_tool_po, unmatched_tool_po]);
    let prompt = builder.build();

    // neural 工具进入神经工具区块
    assert!(prompt.contains("【神经工具】"));
    assert!(prompt.contains("neural.memory_search"));
    // hr 工具匹配 agent role，进入常用工具区块
    assert!(prompt.contains("【常用工具】"));
    assert!(prompt.contains("hr.leave_query"));
    // finance 工具不匹配，不应出现
    assert!(!prompt.contains("finance.budget"));
    assert!(!prompt.contains("预算查询"));
}

/// 验证 trait 风格的链式调用：依次调用多个方法后 build() 结果包含所有部分
#[test]
fn prompt_builder_trait_chained_calls_build_complete_prompt() {
    use crate::models::agent::AgentPo;
    use crate::models::memory::Memory;
    use crate::models::message::Message;

    let agent_po = AgentPo::new(
        "测试助手".to_string(),
        vec!["助手".to_string()],
        "测试".to_string(),
        vec!["能力".to_string()],
        "灵魂".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    let memories: Vec<Memory> = vec![];
    let message = Message::new_with_context(
        "msg-1".to_string(),
        None,
        Some("task-1".to_string()),
        "user-1".to_string(),
        "agent-1".to_string(),
        common::enums::MessageRole::User,
        common::enums::MessageRole::Agent,
        common::enums::MessageType::Text,
        "你好".to_string(),
        None,
        crate::models::file::FileMeta::default(),
        None,
        None,
        None,
        "test".to_string(),
    );

    let mut builder: Box<dyn PromptBuilder> = Box::new(DefaultPromptBuilder::new());
    builder.current_trace_id("trace-001");
    builder.system_prompt(&agent);
    builder.history(&memories);
    builder.current_message(&message);

    let prompt = builder.build();
    assert!(prompt.contains("trace-001"));
    assert!(prompt.contains("测试助手"));
    assert!(prompt.contains("你好"));
    assert!(prompt.contains("请回复："));
}
