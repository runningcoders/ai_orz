//! Memory DAL 单元测试

use crate::error::AppError;
use crate::models::memory::ShortTermMemoryIndexPo;
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::VectorIndexParams;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{new_memory_dao, new_memory_vector_dao, MemoryQuery};
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use crate::service::dal::memory::{new, MemoryDal};
use crate::models::brain::CortexTrait;
use common::enums::MemoryStatus;
use sqlx::SqlitePool;
use std::sync::Arc;

// ========== Mock 实现 ==========

/// Mock ModelProviderDao（返回 None，跳过向量搜索）
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<(), AppError> {
        Ok(())
    }
    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<ModelProviderPo>, AppError> {
        Ok(None)
    }
    async fn query(&self, _ctx: RequestContext, _query: ModelProviderQuery) -> Result<Vec<ModelProviderPo>, AppError> {
        Ok(Vec::new())
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError> {
        Ok(Vec::new())
    }
    async fn update(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_default_embedding_provider(&self, _ctx: RequestContext) -> Result<Option<ModelProviderPo>, AppError> {
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
    async fn prompt(&self, _ctx: RequestContext, _cortex: &dyn CortexTrait, _prompt: &str) -> anyhow::Result<String> {
        Ok("".to_string())
    }
    async fn embed_text_raw(&self, _ctx: RequestContext, _cortex: &dyn CortexTrait, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }
    async fn embed_entity(&self, _ctx: RequestContext, _cortex: &dyn CortexTrait, _entity: &dyn crate::models::vector::Vectorizable) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
        Ok(crate::models::vector::VectorIndexParams {
            vector: Vec::new(),
            content_hash: "".to_string(),
            model_provider_id: "".to_string(),
            embedding_model: "".to_string(),
            expire_at: None,
        })
    }
    async fn embed_text_for_search(&self, _ctx: RequestContext, _cortex: &dyn CortexTrait, _text: &str) -> anyhow::Result<crate::models::vector::VectorIndexParams> {
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
    new(memory_dao, memory_vector_dao, model_provider_dao, cortex_dao)
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
async fn test_query_short_term(pool: SqlitePool) -> Result<(), AppError> {
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
async fn test_query_with_status_filter(pool: SqlitePool) -> Result<(), AppError> {
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
async fn test_query_empty_result(pool: SqlitePool) -> Result<(), AppError> {
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