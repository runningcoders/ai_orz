//! Memory DAO - 记忆系统数据访问
//!\n//! 负责：\n//! - 短期记忆索引的增删查改（SQLite）\n//! - 长期知识图谱节点的增删查改（SQLite）\n//! - 记忆追踪文件的写入（每日文件追加）\n//! - 原始记忆不可修改不可删除，只能追加，符合设计原则\n\nuse async_trait::async_trait;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryTrace,
    MemoryTracePosition, ShortTermMemoryIndexPo,
};
use crate::models::vector::{VectorIndexParams, VectorSearchHit};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::{KnowledgeRelationType, MemoryStatus, MemoryType};
use common::error::Result;

// ==================== 查询参数结构体 ====================

/// 记忆查询的排序方向
///
/// 同一个「取 N 条」在不同场景下语义相反：
/// - 取对话上下文要**最近**的（【历史对话】）
/// - 取待沉淀队列要**最早**的（否则老记忆会被新记忆持续挤出窗口，永远轮不到沉淀）
///
/// # 作用域
/// 目前仅作用于**短期记忆**查询（`query_short_term`）。长期知识节点查询固定按
/// `updated_at DESC`（最近修改优先，便于捞回刚更新的节点），不受此字段影响。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MemorySortOrder {
    /// 最近优先：`ORDER BY created_at DESC`
    ///
    /// 适用于「给我最近 N 条」的上下文类查询。
    #[default]
    RecentFirst,
    /// 最早优先：`ORDER BY created_at ASC`
    ///
    /// 适用于「给我最早未处理的 N 条」的队列类查询（如沉淀队列、待整理队列），
    /// 保证先进先出，不会饿死老数据。
    OldestFirst,
}

impl MemorySortOrder {
    /// 渲染为 SQL 的 ORDER BY 子句（含前导空格）
    pub fn to_sql(&self) -> &'static str {
        match self {
            MemorySortOrder::RecentFirst => " ORDER BY created_at DESC",
            MemorySortOrder::OldestFirst => " ORDER BY created_at ASC",
        }
    }
}

/// ✅ 记忆通用查询参数（用于业务过滤）
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// 按 ID 批量查询（向量搜索的核心过滤）
    pub ids: Option<Vec<String>>,
    /// 按 Agent ID 过滤
    pub agent_id: Option<String>,
    /// 按状态过滤
    pub status: Option<MemoryStatus>,
    /// 排除特定状态（软删除专用）
    pub exclude_status: Option<MemoryStatus>,
    /// 关键词搜索（用于传统 LIKE/MATCH 匹配）
    pub keyword: Option<String>,
    /// 最大返回条数
    pub limit: Option<usize>,
    /// ✅ 按记忆类型过滤
    pub memory_type: Option<MemoryType>,
    /// 按 tags 过滤（OR 语义，命中任一 tag 即可，JSON 数组列）
    pub tags: Option<Vec<String>>,
    /// 按 task_id 过滤（聚焦到特定任务的记忆，注意力机制）
    /// None = 不过滤（默认，取所有任务的最近记忆）
    /// Some(id) = 只取该 task 的记忆
    pub task_id: Option<String>,
    /// 是否包含其他 Agent 共享的 published 节点（默认 false）
    pub include_shared: bool,
    /// 新增：按知识节点类型过滤（summary/concept/fact/procedure）
    /// 注意：与 memory_type 不同——memory_type 是记忆大类型，node_type 是知识节点的子类型
    pub node_type: Option<String>,
    /// 排序方向，默认 [`MemorySortOrder::RecentFirst`]（保持历史行为）
    ///
    /// 队列类查询（如待沉淀短期记忆）应显式指定 [`MemorySortOrder::OldestFirst`]。
    pub order: MemorySortOrder,
}

/// ✅ 记忆搜索统一入参（关键词搜索 + 向量语义搜索共用）
#[derive(Debug, Clone, Default)]
pub struct MemorySearch {
    /// 关键词搜索查询（用于传统全文检索）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// 向量距离阈值，超过此值的结果被过滤。None 表示使用默认值 0.8
    pub vector_distance_threshold: Option<f32>,
    /// ✅ 业务过滤条件（直接复用 MemoryQuery）
    pub filters: MemoryQuery,
}

// ==================== DAO 接口 ====================

/// Memory DAO 接口
///
/// 原始记忆不可修改不可删除，只能追加查询
#[async_trait]
pub trait MemoryDao: Send + Sync {
    /// 追加写入单条 MemoryTrace 到每日 JSONL 文件
    ///
    /// 仅负责文件写入，不写入 SQLite 索引。
    /// 上层（DAL）拿到 position 后，可后续构造 `ShortTermMemoryIndexPo` 并调用
    /// `create_short_term_index` 建立索引，或写入 `KnowledgeReferencePo` 建立引用。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - trace: 记忆追踪
    /// # 返回
    /// - 成功返回 `MemoryTracePosition`（trace_id + 文件名 + 行号）
    /// - 失败返回 Err
    async fn append_trace(
        &self,
        ctx: RequestContext,
        trace: &MemoryTrace,
    ) -> Result<MemoryTracePosition>;

    /// 批量追加多条 MemoryTrace 到每日 JSONL 文件
    ///
    /// 仅负责文件写入，不写入 SQLite 索引。
    /// 顺序写入，按入参顺序返回 position 列表。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - traces: 记忆追踪列表
    /// # 返回
    /// - 成功返回 position 列表（顺序与入参一致）
    /// - 失败返回 Err
    async fn batch_append_traces(
        &self,
        ctx: RequestContext,
        traces: &[MemoryTrace],
    ) -> Result<Vec<MemoryTracePosition>>;

    /// 创建短期记忆索引（仅写 SQLite，不接触文件）
    ///
    /// 由 DAL 层在 trace 落盘后构造完整 `ShortTermMemoryIndexPo`（含 trace_ids、summary、tags 等）
    /// 后调用此方法插入索引表。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - index: 完整短期记忆索引
    /// # 返回
    /// - 成功返回 Ok(())
    /// - 失败返回 Err
    async fn create_short_term_index(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<()>;

    /// 更新短期记忆索引（按 id 全字段覆盖）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - index: 完整短期记忆索引（id 必须存在）
    /// # 返回
    /// - 成功返回 Ok(())
    /// - 记录不存在返回 Err(common::error::Error::not_found)
    async fn update_short_term_index(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<()>;

    /// 根据 ID 查询短期记忆索引
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - id: 记忆 ID（hash）
    /// # 返回
    /// - 找到返回 Some(index)
    /// - 没找到返回 None
    async fn get_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>>;

    /// 查询 Agent 的所有短期记忆索引（按时间倒序）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - limit: 最大返回条数
    /// # 返回
    /// - 索引列表
    async fn list_short_term_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>>;

    /// 通用组合查询短期记忆索引
    async fn query_short_term(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<ShortTermMemoryIndexPo>>;

    /// 全文检索短期记忆索引
    ///
    /// 使用 FTS5 MATCH + BM25 排序，返回匹配的索引及 FTS 相关性评分。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - search: 统一搜索参数（关键词 + 业务过滤）
    /// # 返回
    /// - 匹配的索引列表（按 BM25 相关性排序），每条携带 `fts_rank`（越小越相关）
    async fn search_short_term(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(ShortTermMemoryIndexPo, Option<f32>)>>;

    /// 读取记忆追踪完整内容
    ///
    /// 根据索引中的 date_path + byte_start + byte_length 读取内容
    ///
    /// # 参数
    /// - index: 短期索引
    /// # 返回
    /// - 完整内容字符串
    fn read_memory_content(&self, index: &ShortTermMemoryIndexPo) -> Result<String>;

    /// 遗忘短期记忆索引（软删除，标记为已遗忘）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - id: 索引 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn forget_short_term_index(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // ========== 长期知识图谱相关 ==========

    /// 创建或更新知识节点（upsert）
    ///
    /// 如果节点 ID 已存在则更新，不存在则创建
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node: 知识节点
    /// # 返回
    /// - 成功返回 Ok(())
    async fn save_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<()>;

    /// 更新知识节点（按 id 全字段覆盖）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node: 完整知识节点（id 必须存在）
    /// # 返回
    /// - 成功返回 Ok(())
    /// - 记录不存在返回 Err(common::error::Error::not_found)
    async fn update_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<()>;

    /// 批量创建或更新知识节点（批量 upsert）
    ///
    /// 用于批量更新知识图谱，一次写入多个节点
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - nodes: 节点列表
    /// # 返回
    /// - 成功返回 Ok(())
    async fn batch_save_knowledge_nodes(
        &self,
        ctx: RequestContext,
        nodes: &[LongTermKnowledgeNodePo],
    ) -> Result<()>;

    /// 根据 ID 获取知识节点
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - id: 节点 ID
    /// # 返回
    /// - 找到返回 Some(node), 没找到返回 None
    async fn get_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<LongTermKnowledgeNodePo>>;

    /// 查询 Agent 的所有知识节点
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - node_type: 可选过滤节点类型，None 不过滤
    /// - limit: 最大返回条数
    /// # 返回
    /// - 节点列表
    async fn list_knowledge_nodes_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        node_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>>;

    /// 通用组合查询知识节点
    async fn query_knowledge_nodes(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<LongTermKnowledgeNodePo>>;

    /// 全文检索知识节点
    ///
    /// 使用 FTS5 MATCH + BM25 排序，返回匹配的节点及 FTS 相关性评分。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - search: 统一搜索参数（关键词 + 业务过滤）
    /// # 返回
    /// - 匹配的节点列表（按 BM25 相关性排序），每条携带 `fts_rank`（越小越相关）
    async fn search_knowledge_nodes(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(LongTermKnowledgeNodePo, Option<f32>)>>;

    /// 删除知识节点
    ///
    /// 同时删除相关的引用
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - id: 节点 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_knowledge_node(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 添加知识引用
    ///
    /// 记录知识节点引用了哪些原始短期记忆
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - reference: 引用
    /// # 返回
    /// - 成功返回 Ok(())
    async fn add_knowledge_reference(
        &self,
        ctx: RequestContext,
        reference: &KnowledgeReferencePo,
    ) -> Result<()>;

    /// 批量添加知识引用
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - references: 引用列表
    /// # 返回
    /// - 成功返回 Ok(())
    async fn batch_add_knowledge_references(
        &self,
        ctx: RequestContext,
        references: &[KnowledgeReferencePo],
    ) -> Result<()>;

    /// 获取知识节点的所有引用
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - knowledge_id: 知识节点 ID
    /// # 返回
    /// - 引用列表
    async fn list_knowledge_references(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Vec<KnowledgeReferencePo>>;

    // ========== 知识节点关系相关 ==========

    /// 添加知识节点关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - relation: 关系
    /// # 返回
    /// - 成功返回 Ok(())
    async fn add_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()>;

    /// 批量添加知识节点关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - relations: 关系列表
    /// # 返回
    /// - 成功返回 Ok(())
    async fn batch_add_knowledge_relations(
        &self,
        ctx: RequestContext,
        relations: &[KnowledgeNodeRelationPo],
    ) -> Result<()>;

    /// Upsert 知识节点关系（按 id 冲突更新 source/target/type/updated_at）
    ///
    /// 用于 DAL 层 `update` 方法 — 给 Relation 提供"补充数据"语义。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - relation: 关系 PO（必带 id）
    /// # 返回
    /// - 成功返回 Ok(())
    async fn upsert_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()>;

    /// 获取节点的所有出边关系（从该节点出发）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - source_id: 源节点 ID
    /// # 返回
    /// - 关系列表
    async fn list_outgoing_relations(
        &self,
        ctx: RequestContext,
        source_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>>;

    /// 获取节点的所有入边关系（指向该节点）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - target_id: 目标节点 ID
    /// # 返回
    /// - 关系列表
    async fn list_incoming_relations(
        &self,
        ctx: RequestContext,
        target_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>>;

    /// 获取节点的所有关系（出入边都包含）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node_id: 节点 ID
    /// # 返回
    /// - 关系列表
    async fn list_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>>;

    /// 批量获取多个节点的所有关系（出入边都包含）
    ///
    /// 相当于对每个 node_id 调用 list_all_relations_for_node，然后合并去重。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node_ids: 节点 ID 列表
    /// # 返回
    /// - 关系列表
    async fn list_relations_batch(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>>;

    /// 删除指定关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - relation_id: 关系 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_knowledge_relation(&self, ctx: RequestContext, relation_id: &str)
    -> Result<()>;

    /// 删除节点的所有关系
    ///
    /// 当删除节点时调用，清理所有相关关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node_id: 节点 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_all_relations_for_node(&self, ctx: RequestContext, node_id: &str)
    -> Result<()>;

    /// 查询指定类型的关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - source_id: 源节点 ID
    /// - relation_type: 关系类型
    /// # 返回
    /// - 关系列表
    async fn find_relations_by_type(
        &self,
        ctx: RequestContext,
        source_id: &str,
        relation_type: KnowledgeRelationType,
    ) -> Result<Vec<KnowledgeNodeRelationPo>>;
}

// ==================== MemoryVectorDao Trait ====================

pub use vector::*;

/// ✅ Memory Vector DAO trait - 仅负责记忆向量索引的 CRUD，与基础记忆数据完全解耦
#[async_trait::async_trait]
pub trait MemoryVectorDao: Send + Sync {
    /// 索引短期记忆向量（summary 字段）
    async fn upsert_short_term_vector(
        &self,
        ctx: RequestContext,
        memory_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 索引长期知识节点向量（node_description + summary 拼接）
    async fn upsert_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 语义搜索短期记忆，返回完整的向量行数据 + 相似度距离
    async fn search_short_term_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 语义搜索长期知识节点，返回完整的向量行数据 + 相似度距离
    async fn search_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 获取指定短期记忆的完整向量行数据（包含元信息）
    async fn get_short_term_vector_row(
        &self,
        ctx: RequestContext,
        memory_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;

    /// 获取指定知识节点的完整向量行数据（包含元信息）
    async fn get_knowledge_node_vector_row(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;

    /// 删除短期记忆的向量索引
    async fn delete_short_term_vector(&self, ctx: RequestContext, memory_id: &str) -> Result<()>;

    /// 删除知识节点的向量索引
    async fn delete_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<()>;

    /// 清空所有记忆向量索引
    async fn clear_collection(&self, ctx: RequestContext) -> Result<()>;
}

// ==================== SQLite 实现 ====================

pub mod sqlite;
pub mod vector;

// 子模块构造函数别名（用于 DAL 层组合）
pub use sqlite::{dao as base_dao, init as init_base, new as new_memory_dao};
pub use vector::{dao as vector_dao, init as init_vector, new as new_memory_vector_dao};

/// 统一初始化所有 Memory DAO 单例
pub fn init() {
    init_base();
    init_vector();
}

// ========== 向后兼容：旧代码继续使用 `memory::new()` / `memory::dao()` ==========
pub fn new() -> std::sync::Arc<dyn MemoryDao> {
    new_memory_dao()
}

pub fn dao() -> std::sync::Arc<dyn MemoryDao> {
    base_dao()
}

#[cfg(test)]
mod sqlite_test;
#[cfg(test)]
mod vector_test;
