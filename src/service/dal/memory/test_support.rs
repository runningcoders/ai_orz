//! Memory DAL 测试共享辅助：Mock 实现 + 建库 / 建图 / 建属性定义工具。
//!
//! 覆盖面：
//! - Provider / Cortex / Vector DAO 的 Mock（关键词链路与向量链路两套）
//! - 测试依赖装配：`init_test` / `init_test_with_vector`
//! - 测试库表初始化与 `RequestContext` 构造
//! - 知识图谱造数（遍历图、边方向图）与结果 id 集合辅助
//! - 本体属性定义与物理行计数辅助

use super::{dal, init};
use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, Memory, MemoryCreateParams, MemoryPo,
};
use crate::models::model_provider::ModelProviderPo;
use crate::models::ontology::OntologyPropertyPo;
use crate::models::vector::{VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::memory::{MemoryDal, new};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{
    MemoryQuery, MemoryVectorDao, new_memory_dao, new_memory_vector_dao,
};
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use common::enums::{
    KnowledgeRelationStatus, MemoryStatus, ModelCapability, ModelProviderStatus, ProviderType,
};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

// ========== Mock 实现 ==========

/// Mock ModelProviderDao（返回 None，跳过向量搜索）
pub(super) struct MockModelProviderDao;

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
        _query: ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
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
        Ok(None)
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn get_default_cerebellum_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn find_enabled_decision_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// Mock CortexDao（跳过向量搜索）
pub(super) struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
        _thinking_intensity: Option<common::enums::ThinkingIntensity>,
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
            reasoning_items: Vec::new(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

// ========== 测试辅助函数 ==========

/// 初始化测试依赖（使用真实 DAO + Mock Provider/Cortex）
pub(super) async fn init_test(_pool: SqlitePool) -> Arc<dyn MemoryDal> {
    // 必须先初始化 config（文件操作需要 base_data_path）
    let _ = crate::config::init();

    // 创建真实 DAO 实例
    let memory_dao = new_memory_dao();
    let memory_vector_dao = new_memory_vector_dao();
    // 使用 Mock 跳过向量依赖
    let model_provider_dao: Arc<dyn ModelProviderDao> = Arc::new(MockModelProviderDao);
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexDao);

    // 创建 DAL 实例（三期 direction：MemoryDalImpl 组合 OntologyDao）
    let ontology_dao = crate::service::dao::ontology::new();
    new(
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
        ontology_dao,
    )
}

/// 创建测试用的 RequestContext
pub(super) fn create_test_ctx(pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 初始化数据库表结构
pub(super) async fn init_test_tables(pool: &SqlitePool) {
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
            tags TEXT NOT NULL DEFAULT '[]',
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

// ========== 向量搜索 Mock 实现 ==========

/// Mock ModelProviderDao（返回 dummy embedding provider，触发向量搜索链路）
pub(super) struct MockVectorProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockVectorProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
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
        Ok(Some(ModelProviderPo {
            id: "mock-embedding-provider".to_string(),
            name: "Mock Embedding Provider".to_string(),
            provider_type: ProviderType::OpenAI,
            model_name: "mock-embedding-model".to_string(),
            capability: ModelCapability::Embedding,
            api_key: "mock-key".to_string(),
            base_url: None,
            description: None,
            config: "{}".to_string(),
            status: ModelProviderStatus::Normal,
            created_by: "test".to_string(),
            modified_by: "test".to_string(),
            created_at: 0,
            updated_at: 0,
        }))
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn get_default_cerebellum_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn find_enabled_decision_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// Mock CortexDao（返回固定 dummy 向量，用于向量搜索链路）
pub(super) struct MockCortexVectorDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexVectorDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
        _thinking_intensity: Option<common::enums::ThinkingIntensity>,
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
            reasoning_items: Vec::new(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1, 0.2, 0.3]])
    }
}

/// Mock MemoryVectorDao（返回预设的向量搜索结果）
pub(super) struct MockMemoryVectorDao {
    short_term_hits: Vec<(String, f32)>,
    knowledge_node_hits: Vec<(String, f32)>,
}

pub(super) fn build_vector_search_hits(hits: &[(String, f32)]) -> Vec<VectorSearchHit> {
    hits.iter()
        .map(|(id, dist)| VectorSearchHit {
            row: VectorRow {
                id: id.clone(),
                vector: vec![0.1, 0.2, 0.3],
                payload: crate::models::vector::VectorPayload::default(),
                meta: VectorMeta {
                    content_hash: "mock".to_string(),
                    payload_hash: crate::models::vector::VectorPayload::default().hash(),
                    embedding_model: "mock".to_string(),
                    indexed_at: 0,
                    expire_at: None,
                },
            },
            distance: *dist,
        })
        .collect()
}

#[async_trait::async_trait]
impl MemoryVectorDao for MockMemoryVectorDao {
    async fn upsert_short_term_vector(
        &self,
        _ctx: RequestContext,
        _memory_id: &str,
        _vector_params: &VectorIndexParams,
    ) -> Result<()> {
        Ok(())
    }
    async fn upsert_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        _knowledge_id: &str,
        _vector_params: &VectorIndexParams,
    ) -> Result<()> {
        Ok(())
    }
    async fn search_short_term_vector(
        &self,
        _ctx: RequestContext,
        _query_vector: &[f32],
        _top_k: i32,
        _filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        Ok(build_vector_search_hits(&self.short_term_hits))
    }
    async fn search_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        _query_vector: &[f32],
        _top_k: i32,
        _filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        Ok(build_vector_search_hits(&self.knowledge_node_hits))
    }
    async fn get_short_term_vector_row(
        &self,
        _ctx: RequestContext,
        _memory_id: &str,
    ) -> Result<Option<VectorRow>> {
        Ok(None)
    }
    async fn get_knowledge_node_vector_row(
        &self,
        _ctx: RequestContext,
        _knowledge_id: &str,
    ) -> Result<Option<VectorRow>> {
        Ok(None)
    }
    async fn delete_short_term_vector(&self, _ctx: RequestContext, _memory_id: &str) -> Result<()> {
        Ok(())
    }
    async fn delete_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        _knowledge_id: &str,
    ) -> Result<()> {
        Ok(())
    }
    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        Ok(())
    }
}

/// 初始化测试依赖（带向量搜索 Mock，可配置预设向量命中）
pub(super) async fn init_test_with_vector(
    _pool: SqlitePool,
    short_term_hits: Vec<(String, f32)>,
    knowledge_node_hits: Vec<(String, f32)>,
) -> Arc<dyn MemoryDal> {
    let _ = crate::config::init();
    let memory_dao = new_memory_dao();
    let memory_vector_dao: Arc<dyn MemoryVectorDao> = Arc::new(MockMemoryVectorDao {
        short_term_hits,
        knowledge_node_hits,
    });
    let model_provider_dao: Arc<dyn ModelProviderDao> = Arc::new(MockVectorProviderDao);
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexVectorDao);
    new(
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
        crate::service::dao::ontology::new(),
    )
}

// ========== 知识图谱遍历造数辅助 ==========

/// 遍历测试节点工厂（归属 agent-001，与测试 ctx 的 agent_id 匹配）
pub(super) fn traverse_node(id: &str, now: i64) -> LongTermKnowledgeNodePo {
    LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "agent-001".to_string(),
        node_name: format!("节点 {}", id),
        node_description: format!("节点 {} 描述", id),
        node_type: "concept".to_string(),
        summary: format!("节点 {} 总结", id),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    }
}

/// 遍历测试关系工厂
pub(super) fn traverse_rel(
    id: &str,
    src: &str,
    tgt: &str,
    created_at: i64,
) -> KnowledgeNodeRelationPo {
    KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: src.to_string(),
        target_node_id: tgt.to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at,
        updated_at: created_at,
    }
}

/// 建节点 + 建关系的测试前置（ctx 需带 agent_id=agent-001）
pub(super) async fn seed_graph(
    dal: &Arc<dyn MemoryDal>,
    ctx: &RequestContext,
    nodes: Vec<LongTermKnowledgeNodePo>,
    relations: Vec<KnowledgeNodeRelationPo>,
) -> Result<()> {
    for node in nodes {
        dal.create(
            ctx.clone(),
            MemoryCreateParams::CreateKnowledgeNode {
                node,
                references: vec![],
            },
        )
        .await?;
    }
    if !relations.is_empty() {
        dal.create(ctx.clone(), MemoryCreateParams::CreateRelations(relations))
            .await?;
    }
    Ok(())
}

pub(super) fn node_ids_of(results: &[Memory]) -> std::collections::HashSet<String> {
    results
        .iter()
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn rel_ids_of(results: &[Memory]) -> std::collections::HashSet<String> {
    results
        .iter()
        .filter_map(|m| match &m.po {
            MemoryPo::Relation(r) => Some(r.id.clone()),
            _ => None,
        })
        .collect()
}

// ========== 知识图谱 / 属性造数辅助 ==========

pub(super) fn node_po(id: &str, agent_id: &str) -> LongTermKnowledgeNodePo {
    LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: agent_id.to_string(),
        node_name: format!("节点-{id}"),
        node_description: String::new(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }
}

pub(super) fn relation_po(id: &str, source: &str, target: &str) -> KnowledgeNodeRelationPo {
    KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    }
}

/// 造数：2 个活跃知识节点 + 1 条生效边 + 1 条悬挂边（关系表无外键，悬挂边可直接插入）
pub(super) async fn seed_graph_ctx(pool: SqlitePool) -> RequestContext {
    // dal::init() 会取用四个 DAO 单例，先按依赖顺序逐个初始化
    //（生产路径由 bootstrap 统一完成，测试路径须自备；OnceLock.set 幂等可重复调）
    crate::service::dao::memory::init();
    crate::service::dao::memory::init_vector();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    // 三期 direction：get_knowledge_graph 聚合依赖 OntologyDao 单例
    crate::service::dao::ontology::init();
    init();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool.clone());
    let d = dal();
    for id in ["kn_a", "kn_b"].iter() {
        d.create(
            ctx.clone(),
            MemoryCreateParams::CreateKnowledgeNode {
                node: node_po(id, "agent_x"),
                references: vec![],
            },
        )
        .await
        .expect("create knowledge node should succeed");
    }
    d.create(
        ctx.clone(),
        MemoryCreateParams::CreateRelations(vec![
            // 生效边：kn_a → kn_b（走生产写入路径）
            relation_po("kr_ab", "kn_a", "kn_b"),
        ]),
    )
    .await
    .expect("create relations should succeed");
    // 悬挂边：target "kn_ghost" 不在活跃节点集（R2 应被过滤）。
    // DAO 写入侧已在事务内拦截悬空端点（建边防线），这里模拟的是
    // 「防线存在前的存量脏数据」，故绕过生产路径直接裸 SQL 植入。
    sqlx::query(
        r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
) VALUES ('kr_ghost', 'kn_a', 'kn_ghost', 'related', NULL, 1, 0, 0)
"#,
    )
    .execute(&pool)
    .await
    .expect("seed dangling edge should succeed");
    ctx
}

/// 造属性定义（TBox 直接插 DAO：管理页写入路径已在 Domain 层覆盖）
pub(super) fn property_po(
    term_key: &str,
    data_type: &str,
    value_range: &str,
    cardinality: &str,
    applies_to: &str,
) -> OntologyPropertyPo {
    OntologyPropertyPo::new(
        term_key,
        format!("属性-{term_key}"),
        format!("{term_key} 的语义描述"),
        "[]",
        applies_to,
        data_type,
        value_range,
        cardinality,
        None,
        false,
    )
}

pub(super) async fn define_property(ctx: &RequestContext, po: &OntologyPropertyPo) {
    crate::service::dao::ontology::dao()
        .insert_property(ctx.clone(), po)
        .await
        .expect("insert property definition should succeed");
}

/// 含历史版本的物理行数（版本化「保留全历史」的直接证据）
pub(super) async fn count_property_rows(ctx: &RequestContext, target_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_property WHERE target_id = ?")
        .bind(target_id)
        .fetch_one(ctx.db_pool())
        .await
        .expect("count knowledge_property rows")
}
