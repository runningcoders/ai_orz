//! Memory DAO SQLite 实现——工厂 + 单例 + 跨面共享 helper
//!
//! 按业务面拆分为多个子模块（对齐 `dal/agent`、`dal/organization` 的目录模式）：
//!
//! - **本文件**：[`MemoryDaoSqliteImpl`] 结构 + 单例管理 + `pool` / `agent_memory_dir` 共享 helper
//! - [`trace`]：记忆轨迹写入与 JSONL 扫描查询
//! - [`short_term`]：短期记忆索引 CRUD / 查询 / FTS5 搜索
//! - [`knowledge`]：长期知识节点 CRUD + 记忆引用
//! - [`relations`]：知识节点关系 CRUD / 批量 / 端点校验
//! - [`properties`]：知识属性写入 / 生效行查询 / 历史降级
//! - [`drift`]：本体词频聚合与漂移下钻
//!
//! 原 trait 实现方法分散到子模块的固有 `impl` 中（Rust 不允许同一 trait 对同一类型
//! 有多个 impl 块），由本文件的 `memory_dao_impl!` 生成 1:1 转发的唯一 trait impl，
//! 所有业务逻辑仍在对应子模块内。

use super::{DEFAULT_MEMORY_TRACE_QUERY_LIMIT, MAX_MEMORY_TRACE_QUERY_LIMIT, MemoryDao};
use crate::config;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgePropertyPo, KnowledgeReferencePo, LongTermKnowledgeNodePo,
    MemoryTrace, MemoryTracePosition, ShortTermMemoryIndexPo,
};
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::service::dao::memory::{
    DriftClassDetailRow, DriftRelationDetailRow, MemoryQuery, MemorySearch, MemoryTraceQuery,
    PropertyDomainFreqRow, PropertyValueFreqRow, TermFrequencyRow,
};
use async_trait::async_trait;
use common::api::{PagedResult, PaginationParams};
use common::enums::ontology::PropertyTargetType;
use common::error::Result;
use common::ontology_property::PropertyFilter;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// SQLite 默认绑定参数上限为 999（SQLITE_MAX_VARIABLE_NUMBER）。
/// `list_relations_batch` 每个节点 ID 占 2 个绑定（source + target 两个 IN 列表），
/// 按 400 个节点分块（800 绑定）留出安全余量；ids IN 单绑定场景同样复用此常量。
pub const IN_CLAUSE_CHUNK: usize = 400;

/// 词频聚合返回行数的防御上限：正常本体词表远小于该值，
/// 防御脏数据（海量不同 node_type / relation_type）导致 dashboard 全量聚合行数爆炸
pub const MAX_WORD_FREQ_ROWS: i64 = 1000;

mod drift;
mod knowledge;
mod properties;
mod property_effectiveness;
mod relations;
mod short_term;
mod trace;

#[cfg(test)]
pub(crate) use trace::{query_traces_at, resolve_trace_positions_at};

#[cfg(test)]
mod drift_test;
#[cfg(test)]
mod fts_test;
#[cfg(test)]
mod knowledge_test;
#[cfg(test)]
mod relations_test;
#[cfg(test)]
mod short_term_test;
#[cfg(test)]
mod trace_test;

// ==================== 工厂方法 + 单例 ====================

static MEMORY_DAO: OnceLock<Arc<dyn super::MemoryDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Memory DAO 实例（用于测试）
pub fn new() -> Arc<dyn super::MemoryDao + Send + Sync> {
    Arc::new(MemoryDaoSqliteImpl::new())
}

/// 获取 Memory DAO 单例
pub fn dao() -> Arc<dyn super::MemoryDao + Send + Sync> {
    MEMORY_DAO.get().cloned().unwrap()
}

/// 初始化 Memory DAO 单例
pub fn init() {
    let _ = MEMORY_DAO.set(new());
}

/// SQLite Memory DAO 实现
#[derive(Default)]
pub struct MemoryDaoSqliteImpl;

impl MemoryDaoSqliteImpl {
    /// 创建新的 DAO 实例
    pub fn new() -> Self {
        MemoryDaoSqliteImpl
    }

    /// 获取 Agent 记忆目录完整路径（用于写入）
    fn agent_memory_dir(&self, agent_id: &str) -> PathBuf {
        let base = config::get().base_data_path();
        paths::agent_memory_dir(&base, agent_id)
    }

    /// 获取连接池从上下文
    fn pool(&self, ctx: RequestContext) -> SqlitePool {
        ctx.db_pool().clone()
    }
}

// ==================== MemoryDao 实现（业务实现分布于子模块的固有 impl） ====================

macro_rules! memory_dao_impl {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty),* $(,)?) => {
        #[async_trait]
        impl MemoryDao for MemoryDaoSqliteImpl {
            $(
                async fn $name(&self, $($arg: $ty),*) -> $ret {
                    MemoryDaoSqliteImpl::$name(self, $($arg),*).await
                }
            )*

            fn read_memory_content(&self, index: &ShortTermMemoryIndexPo) -> Result<String> {
                MemoryDaoSqliteImpl::read_memory_content(self, index)
            }
        }
    };
}

memory_dao_impl! {
    append_trace(ctx: RequestContext, trace: &MemoryTrace) -> Result<MemoryTracePosition>,
    batch_append_traces(ctx: RequestContext, traces: &[MemoryTrace]) -> Result<Vec<MemoryTracePosition>>,
    query_traces(ctx: RequestContext, query: MemoryTraceQuery) -> Result<PagedResult<MemoryTrace>>,
    get_trace(ctx: RequestContext, agent_id: &str, trace_id: &str) -> Result<Option<MemoryTrace>>,
    resolve_trace_positions(ctx: RequestContext, agent_id: &str, trace_ids: &[String]) -> Result<HashMap<String, MemoryTracePosition>>,
    create_short_term_index(ctx: RequestContext, index: ShortTermMemoryIndexPo) -> Result<()>,
    update_short_term_index(ctx: RequestContext, index: ShortTermMemoryIndexPo) -> Result<()>,
    forget_short_term_index(ctx: RequestContext, id: &str) -> Result<()>,
    get_short_term_index(ctx: RequestContext, id: &str) -> Result<Option<ShortTermMemoryIndexPo>>,
    list_short_term_by_agent(ctx: RequestContext, agent_id: &str, limit: usize) -> Result<Vec<ShortTermMemoryIndexPo>>,
    query_short_term(ctx: RequestContext, query: MemoryQuery) -> Result<Vec<ShortTermMemoryIndexPo>>,
    search_short_term(ctx: RequestContext, search: MemorySearch) -> Result<Vec<(ShortTermMemoryIndexPo, Option<f32>)>>,
    save_knowledge_node(ctx: RequestContext, node: &LongTermKnowledgeNodePo) -> Result<()>,
    update_knowledge_node(ctx: RequestContext, node: &LongTermKnowledgeNodePo) -> Result<()>,
    batch_save_knowledge_nodes(ctx: RequestContext, nodes: &[LongTermKnowledgeNodePo]) -> Result<()>,
    get_knowledge_node(ctx: RequestContext, id: &str) -> Result<Option<LongTermKnowledgeNodePo>>,
    list_knowledge_nodes_by_agent(ctx: RequestContext, agent_id: &str, node_type: Option<&str>, limit: usize) -> Result<Vec<LongTermKnowledgeNodePo>>,
    query_knowledge_nodes(ctx: RequestContext, query: MemoryQuery) -> Result<Vec<LongTermKnowledgeNodePo>>,
    search_knowledge_nodes(ctx: RequestContext, search: MemorySearch) -> Result<Vec<(LongTermKnowledgeNodePo, Option<f32>)>>,
    delete_knowledge_node(ctx: RequestContext, id: &str) -> Result<()>,
    add_knowledge_reference(ctx: RequestContext, reference: &KnowledgeReferencePo) -> Result<()>,
    batch_add_knowledge_references(ctx: RequestContext, references: &[KnowledgeReferencePo]) -> Result<()>,
    list_knowledge_references(ctx: RequestContext, knowledge_id: &str) -> Result<Vec<KnowledgeReferencePo>>,
    add_knowledge_relation(ctx: RequestContext, relation: &KnowledgeNodeRelationPo) -> Result<()>,
    batch_add_knowledge_relations(ctx: RequestContext, relations: &[KnowledgeNodeRelationPo]) -> Result<()>,
    upsert_knowledge_relation(ctx: RequestContext, relation: &KnowledgeNodeRelationPo) -> Result<()>,
    list_outgoing_relations(ctx: RequestContext, source_id: &str) -> Result<Vec<KnowledgeNodeRelationPo>>,
    list_incoming_relations(ctx: RequestContext, target_id: &str) -> Result<Vec<KnowledgeNodeRelationPo>>,
    list_all_relations_for_node(ctx: RequestContext, node_id: &str) -> Result<Vec<KnowledgeNodeRelationPo>>,
    list_relations_batch(ctx: RequestContext, node_ids: &[String]) -> Result<Vec<KnowledgeNodeRelationPo>>,
    query_knowledge_relations(ctx: RequestContext, query: MemoryQuery) -> Result<Vec<KnowledgeNodeRelationPo>>,
    delete_knowledge_relation(ctx: RequestContext, relation_id: &str) -> Result<()>,
    delete_all_relations_for_node(ctx: RequestContext, node_id: &str) -> Result<()>,
    find_relations_by_type(ctx: RequestContext, source_id: &str, relation_type: &str) -> Result<Vec<KnowledgeNodeRelationPo>>,
    upsert_property(ctx: RequestContext, property: &KnowledgePropertyPo) -> Result<()>,
    batch_upsert_properties(ctx: RequestContext, properties: &[KnowledgePropertyPo]) -> Result<()>,
    list_active_properties(ctx: RequestContext, target_type: PropertyTargetType, target_id: &str) -> Result<Vec<KnowledgePropertyPo>>,
    list_active_properties_batch(ctx: RequestContext, targets: &[(PropertyTargetType, String)]) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>>,
    supersede_properties(ctx: RequestContext, target_type: PropertyTargetType, target_id: &str, term_key: &str) -> Result<u64>,
    query_properties_by_value(ctx: RequestContext, filter: &PropertyFilter) -> Result<Vec<KnowledgePropertyPo>>,
    word_freq_relations(ctx: RequestContext, agent_id: Option<String>) -> Result<Vec<TermFrequencyRow>>,
    word_freq_nodes(ctx: RequestContext, agent_id: Option<String>) -> Result<Vec<TermFrequencyRow>>,
    property_domain_freq(ctx: RequestContext) -> Result<Vec<PropertyDomainFreqRow>>,
    property_value_freq(ctx: RequestContext, term_key: Option<String>, node_type: Option<String>) -> Result<Vec<PropertyValueFreqRow>>,
    detail_relations_by_term(ctx: RequestContext, raw_term: &str, agent_id: Option<String>, pagination: PaginationParams) -> Result<PagedResult<DriftRelationDetailRow>>,
    detail_nodes_by_term(ctx: RequestContext, raw_term: &str, agent_id: Option<String>, pagination: PaginationParams) -> Result<PagedResult<DriftClassDetailRow>>,
    publish_nodes_by_type(ctx: RequestContext, node_type_normalized: &str) -> Result<u64>,
}
