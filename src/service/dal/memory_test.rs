//! Memory DAL 单元测试

use common::error::Error;
use crate::models::brain::CortexTrait;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, Memory,
    MemoryCreateParams, MemoryPo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::VectorIndexParams;
use crate::pkg::request_context::RequestContext;
use crate::service::dal::memory::{MemoryDal, new};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{
    MemoryQuery, MemorySearch, new_memory_dao, new_memory_vector_dao,
};
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use common::enums::MemoryStatus;
use sqlx::SqlitePool;
use std::sync::Arc;
use common::error::Result;
use common::bail_err;

// ========== Mock 实现 ==========

/// Mock ModelProviderDao（返回 None，跳过向量搜索）
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _ctx: RequestContext,
        _id: &str,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: ModelProviderQuery,
    ) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
    }
    async fn update(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> Result<()> {
        Ok(())
    }
    async fn delete(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> Result<()> {
        Ok(())
    }
    async fn get_default_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// Mock CortexDao（跳过向量搜索）
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    fn create_cortex_trait(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _rig_tools: Vec<Box<dyn ::rig::tool::ToolDyn>>,
    ) -> anyhow::Result<Box<dyn CortexTrait + Send + Sync>> {
        panic!("MockCortexDao::create_cortex_trait not implemented for tests");
    }
    async fn prompt(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _prompt: &str,
    ) -> anyhow::Result<String> {
        Ok("".to_string())
    }
    async fn embed_text_raw(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _text: &str,
    ) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }
    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        Ok(crate::models::vector::VectorIndexParams {
            vector: Vec::new(),
            content_hash: "".to_string(),
            model_provider_id: "".to_string(),
            embedding_model: "".to_string(),
            expire_at: None,
        })
    }
    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _cortex: &dyn CortexTrait,
        _text: &str,
    ) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        Ok(crate::models::vector::VectorIndexParams {
            vector: Vec::new(),
            content_hash: "".to_string(),
            model_provider_id: "".to_string(),
            embedding_model: "".to_string(),
            expire_at: None,
        })
    }
}

// ========== 测试辅助函数 ==========

/// 初始化测试依赖（使用真实 DAO + Mock Provider/Cortex）
async fn init_test(_pool: SqlitePool) -> Arc<dyn MemoryDal> {
    // 必须先初始化 config（文件操作需要 base_data_path）
    let _ = crate::config::init();

    // 创建真实 DAO 实例
    let memory_dao = new_memory_dao();
    let memory_vector_dao = new_memory_vector_dao();
    // 使用 Mock 跳过向量依赖
    let model_provider_dao: Arc<dyn ModelProviderDao> = Arc::new(MockModelProviderDao);
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexDao);

    // 创建 DAL 实例
    new(
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
    )
}

/// 创建测试用的 RequestContext
fn create_test_ctx(pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple("test-user", pool)
}

/// 初始化数据库表结构
async fn init_test_tables(pool: &SqlitePool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS short_term_memories (
            id TEXT PRIMARY KEY NOT NULL,
            agent_id TEXT NOT NULL,
            task_id TEXT,
            role TEXT NOT NULL,
            summary TEXT NOT NULL,
            tags TEXT NOT NULL DEFAULT '[]',
            trace_ids TEXT NOT NULL DEFAULT '[]',
            status INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
            updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
        );
        CREATE INDEX IF NOT EXISTS idx_short_term_agent_id ON short_term_memories(agent_id);
        CREATE INDEX IF NOT EXISTS idx_short_term_status ON short_term_memories(status);
        
        CREATE TABLE IF NOT EXISTS long_term_knowledge_nodes (
            id TEXT PRIMARY KEY NOT NULL,
            agent_id TEXT NOT NULL,
            node_name TEXT NOT NULL,
            node_description TEXT NOT NULL,
            node_type TEXT NOT NULL DEFAULT 'concept',
            summary TEXT NOT NULL,
            status INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
            updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
        );
        CREATE INDEX IF NOT EXISTS idx_long_term_agent_id ON long_term_knowledge_nodes(agent_id);
        CREATE INDEX IF NOT EXISTS idx_long_term_status ON long_term_knowledge_nodes(status);
        
        CREATE TABLE IF NOT EXISTS knowledge_node_relations (
            id TEXT PRIMARY KEY NOT NULL,
            source_node_id TEXT NOT NULL,
            target_node_id TEXT NOT NULL,
            relation_type TEXT NOT NULL DEFAULT 'related',
            metadata TEXT,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_source_node ON knowledge_node_relations(source_node_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_target_node ON knowledge_node_relations(target_node_id);
        
        CREATE TABLE IF NOT EXISTS knowledge_references (
            id TEXT PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            reference_type TEXT NOT NULL,
            reference_content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
        );
        CREATE INDEX IF NOT EXISTS idx_reference_node_id ON knowledge_references(node_id);
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
}

// ========== 测试用例 ==========

#[sqlx::test]
async fn test_query_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建短期记忆（直接写入数据库绕过 create 的向量依赖）
    let po = ShortTermMemoryIndexPo {
        id: "test-mem-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Hello, world!".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po.id)
    .bind(&po.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po.role)
    .bind(&po.summary)
    .bind(&po.tags)
    .bind(&po.trace_ids)
    .bind(po.status as i32)
    .bind(po.created_at)
    .bind(po.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 查询测试
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;

    assert!(!results.is_empty());

    Ok(())
}

#[sqlx::test]
async fn test_query_with_status_filter(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建一个 Active 状态的记忆
    let po = ShortTermMemoryIndexPo {
        id: "test-mem-active".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Active memory".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po.id)
    .bind(&po.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po.role)
    .bind(&po.summary)
    .bind(&po.tags)
    .bind(&po.trace_ids)
    .bind(po.status as i32)
    .bind(po.created_at)
    .bind(po.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 创建一个 Deleted 状态的记忆
    let po2 = ShortTermMemoryIndexPo {
        id: "test-mem-deleted".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Deleted memory".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Forgotten,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po2.id)
    .bind(&po2.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po2.role)
    .bind(&po2.summary)
    .bind(&po2.tags)
    .bind(&po2.trace_ids)
    .bind(po2.status as i32)
    .bind(po2.created_at)
    .bind(po2.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 查询 Active 状态（默认）
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx.clone(), query).await?;
    assert_eq!(results.len(), 1);

    // 查询 Deleted 状态
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        status: Some(MemoryStatus::Forgotten),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;
    assert_eq!(results.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_query_empty_result(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 查询不存在的 agent
    let query = MemoryQuery {
        agent_id: Some("non-existent-agent".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;

    assert!(results.is_empty());

    Ok(())
}

// ========== create 方法测试 ==========

#[sqlx::test]
async fn test_create_append_traces(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建测试用的 MemoryTrace
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "这是一条测试记忆内容".to_string(),
        None,
    );

    let params = MemoryCreateParams::AppendTraces(vec![trace]);
    let results = dal.create(ctx, params).await?;

    assert!(!results.is_empty());
    // 验证返回的是 Trace 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::Trace(t) => {
            assert_eq!(t.agent_id, "agent-001");
            assert_eq!(t.input, "这是一条测试记忆内容");
            assert!(t.position.is_some()); // 写入后应该有位置信息
        }
        _ => panic!("预期返回 Trace 类型"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_create_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建短期记忆索引
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-test-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "测试短期记忆摘要".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateShortTerm(index);
    let results = dal.create(ctx.clone(), params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 ShortTerm 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::ShortTerm(st) => {
            assert_eq!(st.id, "st-test-001");
            assert_eq!(st.agent_id, "agent-001");
            assert_eq!(st.summary, "测试短期记忆摘要");
        }
        _ => panic!("预期返回 ShortTerm 类型"),
    }

    // 验证可以通过 query 查到
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let query_results = dal.query(ctx, query).await?;
    assert_eq!(query_results.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_create_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建知识节点
    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "kn-test-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "测试知识节点".to_string(),
        node_description: "这是一个测试知识节点的描述".to_string(),
        node_type: "concept".to_string(),
        summary: "测试知识节点的总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 没有引用的情况
    let params = MemoryCreateParams::CreateKnowledgeNode {
        node,
        references: vec![],
    };
    let results = dal.create(ctx.clone(), params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 KnowledgeNode 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::KnowledgeNode(kn) => {
            assert_eq!(kn.id, "kn-test-001");
            assert_eq!(kn.agent_id, "agent-001");
            assert_eq!(kn.node_name, "测试知识节点");
        }
        _ => panic!("预期返回 KnowledgeNode 类型"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_create_relations(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建两个知识节点
    let now = chrono::Utc::now().timestamp();
    let node1 = LongTermKnowledgeNodePo {
        id: "kn-source-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "源节点".to_string(),
        node_description: "源节点描述".to_string(),
        node_type: "concept".to_string(),
        summary: "源节点总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let node2 = LongTermKnowledgeNodePo {
        id: "kn-target-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "目标节点".to_string(),
        node_description: "目标节点描述".to_string(),
        node_type: "concept".to_string(),
        summary: "目标节点总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 先创建节点
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node1,
            references: vec![],
        },
    )
    .await?;
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node2,
            references: vec![],
        },
    )
    .await?;

    // 创建关系
    let relation = KnowledgeNodeRelationPo {
        id: "rel-test-001".to_string(),
        source_node_id: "kn-source-001".to_string(),
        target_node_id: "kn-target-001".to_string(),
        relation_type: common::enums::KnowledgeRelationType::Related,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateRelations(vec![relation]);
    let results = dal.create(ctx, params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 Relation 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::Relation(r) => {
            assert_eq!(r.id, "rel-test-001");
            assert_eq!(r.source_node_id, "kn-source-001");
            assert_eq!(r.target_node_id, "kn-target-001");
        }
        _ => panic!("预期返回 Relation 类型"),
    }

    Ok(())
}

// ========== delete 方法测试 ==========

#[sqlx::test]
async fn test_delete_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建短期记忆
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-delete-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "待删除的短期记忆".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateShortTerm(index.clone());
    dal.create(ctx.clone(), params).await?;

    // 验证存在
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let before_delete = dal.query(ctx.clone(), query.clone()).await?;
    assert_eq!(before_delete.len(), 1);

    // 执行删除
    let memory = Memory::new(MemoryPo::ShortTerm(index));
    dal.delete(ctx.clone(), memory).await?;

    // 验证删除后状态为 Forgotten（软删除）
    let after_delete = dal.query(ctx, query).await?;
    // 默认不包含 Forgotten 状态的记录
    assert_eq!(after_delete.len(), 0);

    Ok(())
}

#[sqlx::test]
async fn test_delete_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建知识节点
    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "kn-delete-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "待删除的知识节点".to_string(),
        node_description: "待删除的描述".to_string(),
        node_type: "concept".to_string(),
        summary: "待删除的总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateKnowledgeNode {
        node: node.clone(),
        references: vec![],
    };
    dal.create(ctx.clone(), params).await?;

    // 直接删除知识节点（测试主要验证删除操作不报错）
    let memory = Memory::new(MemoryPo::KnowledgeNode(node));
    dal.delete(ctx.clone(), memory).await?;

    // 验证查询 Forgotten 状态时能找到 1 条（软删除已生效，状态更新正确）
    let query_forgotten = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        status: Some(MemoryStatus::Forgotten),
        ..Default::default()
    };
    let after_delete = dal.query(ctx, query_forgotten).await?;
    assert_eq!(after_delete.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_delete_trace_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Trace 类型的 Memory
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "测试内容".to_string(),
        None,
    );
    let memory = Memory::new(MemoryPo::Trace(trace));

    // 尝试删除应该返回错误
    let result = dal.delete(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("原始记忆 Trace 不可删除"));

    Ok(())
}

#[sqlx::test]
async fn test_search_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建多条短期记忆
    let now = chrono::Utc::now().timestamp();

    let index1 = ShortTermMemoryIndexPo {
        id: "st-search-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "用户询问 Rust 编程基础概念".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let index2 = ShortTermMemoryIndexPo {
        id: "st-search-002".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: "Python 数据处理与机器学习入门指南".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let index3 = ShortTermMemoryIndexPo {
        id: "st-search-003".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: "其他无关内容".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index1))
        .await?;
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index2))
        .await?;
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index3))
        .await?;

    // 搜索：匹配 "Rust" 关键词（只匹配第一条）
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results.len(), 1);

    // 搜索：匹配 "Python" 关键词（只匹配第二条）
    let results2 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Python".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);

    // 搜索：无匹配
    let results3 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("nonexistent-keyword".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results3.len(), 0);

    Ok(())
}

#[sqlx::test]
async fn test_search_knowledge_nodes(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建多个知识节点
    let now = chrono::Utc::now().timestamp();

    let node1 = LongTermKnowledgeNodePo {
        id: "kn-search-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Rust 所有权机制".to_string(),
        node_description: "深入理解 Rust 所有权、借用和生命周期概念".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 语言的核心特性之一，确保内存安全".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let node2 = LongTermKnowledgeNodePo {
        id: "kn-search-002".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Python 装饰器详解".to_string(),
        node_description: "Python 高级特性：函数装饰器原理与应用".to_string(),
        node_type: "concept".to_string(),
        summary: "Python 装饰器是一种强大的元编程工具".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let node3 = LongTermKnowledgeNodePo {
        id: "kn-search-003".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "其他内容".to_string(),
        node_description: "无关描述".to_string(),
        node_type: "other".to_string(),
        summary: "无关总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node1,
            references: vec![],
        },
    )
    .await?;
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node2,
            references: vec![],
        },
    )
    .await?;
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node3,
            references: vec![],
        },
    )
    .await?;

    // 搜索：按 node_name 匹配 "Rust"
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results.len(), 1);

    // 搜索：按 summary 匹配 "装饰器"
    let results2 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("装饰器".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);

    // 搜索：无匹配
    let results3 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("nonexistent-keyword".to_string()),
                query_vector: None,
                top_k: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results3.len(), 0);

    Ok(())
}

#[sqlx::test]
async fn test_update_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建短期记忆
    let now = chrono::Utc::now().timestamp();
    let original_index = ShortTermMemoryIndexPo {
        id: "st-update-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "原始内容".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateShortTerm(original_index.clone()),
    )
    .await?;

    // 修改内容
    let mut updated_index = original_index.clone();
    updated_index.summary = "更新后的内容".to_string();
    updated_index.tags = "[\"重要\"]".to_string();

    let updated_memory = Memory::new(MemoryPo::ShortTerm(updated_index.clone()));
    let result = dal.update(ctx.clone(), updated_memory).await?;

    // 验证更新成功
    match &result.po {
        MemoryPo::ShortTerm(po) => {
            assert_eq!(po.summary, "更新后的内容");
            assert_eq!(po.tags, "[\"重要\"]");
        }
        _ => panic!("Expected ShortTerm"),
    }

    // 验证数据库中确实更新了
    let query_result = dal
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-001".to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(query_result.len(), 1);
    match &query_result[0].po {
        MemoryPo::ShortTerm(po) => {
            assert_eq!(po.summary, "更新后的内容");
        }
        _ => panic!("Expected ShortTerm"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_update_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建知识节点
    let now = chrono::Utc::now().timestamp();
    let original_node = LongTermKnowledgeNodePo {
        id: "kn-update-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "原始节点名称".to_string(),
        node_description: "原始描述".to_string(),
        node_type: "concept".to_string(),
        summary: "原始总结".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: original_node.clone(),
            references: vec![],
        },
    )
    .await?;

    // 修改内容
    let mut updated_node = original_node.clone();
    updated_node.node_name = "更新后的节点名称".to_string();
    updated_node.node_description = "更新后的描述".to_string();
    updated_node.summary = "更新后的总结".to_string();

    let updated_memory = Memory::new(MemoryPo::KnowledgeNode(updated_node.clone()));
    let result = dal.update(ctx.clone(), updated_memory).await?;

    // 验证更新成功
    match &result.po {
        MemoryPo::KnowledgeNode(po) => {
            assert_eq!(po.node_name, "更新后的节点名称");
            assert_eq!(po.node_description, "更新后的描述");
            assert_eq!(po.summary, "更新后的总结");
        }
        _ => panic!("Expected KnowledgeNode"),
    }

    // 验证数据库中确实更新了
    let query_result = dal
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-001".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(query_result.len(), 1);
    match &query_result[0].po {
        MemoryPo::KnowledgeNode(po) => {
            assert_eq!(po.node_name, "更新后的节点名称");
        }
        _ => panic!("Expected KnowledgeNode"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_update_trace_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Trace 类型的 Memory
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "原始内容".to_string(),
        None,
    );
    let memory = Memory::new(MemoryPo::Trace(trace));

    // 尝试更新应该返回错误
    let result = dal.update(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("原始记忆 Trace 不可修改"));

    Ok(())
}

#[sqlx::test]
async fn test_update_relation_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Relation 类型的 Memory（使用 PO 直接构造）
    let now = chrono::Utc::now().timestamp();
    let relation = KnowledgeNodeRelationPo {
        id: "rel-001".to_string(),
        source_node_id: "node-a".to_string(),
        target_node_id: "node-b".to_string(),
        relation_type: common::enums::KnowledgeRelationType::Related,
        created_at: now,
        updated_at: now,
    };
    let memory = Memory::new(MemoryPo::Relation(relation));

    // 尝试更新应该返回错误
    let result = dal.update(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("记忆 Relation 不可修改，需删除后重建"));

    Ok(())
}
