//! Memory DAO - 记忆系统数据访问
//!
//! 负责：
//! - 短期记忆索引的增删查改（SQLite）
//! - 长期知识图谱节点的增删查改（SQLite）
//! - 记忆追踪文件的写入（每日文件追加）
//! - 原始记忆不可修改不可删除，只能追加，符合设计原则

use crate::error::AppError;
use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo, LongTermKnowledgeNodePo, KnowledgeReferencePo, KnowledgeNodeRelationPo, KnowledgeRelationType};
use crate::pkg::RequestContext;
use async_trait::async_trait;

// ==================== DAO 接口 ====================

/// Memory DAO 接口
///
/// 原始记忆不可修改不可删除，只能追加查询
#[async_trait]
pub trait MemoryDaoTrait: Send + Sync {
    /// 追加写入记忆追踪到每日文件，并插入短期索引
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - trace: 记忆追踪
    /// - summary: 归纳摘要（用于检索）
    /// - tags: 标签列表
    /// # 返回
    /// - 成功返回 Ok(())
    /// - 失败返回 Err
    async fn append_memory_trace(
        &self,
        ctx: RequestContext,
        trace: &MemoryTrace,
        summary: String,
        tags: Vec<String>,
    ) -> Result<ShortTermMemoryIndexPo, AppError>;

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
    ) -> Result<Option<ShortTermMemoryIndexPo>, AppError>;

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
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;

    /// 全文检索短期记忆索引
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - query: 搜索关键词
    /// - limit: 最大返回条数
    /// # 返回
    /// - 匹配的索引列表（按相关性排序）
    async fn search_short_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;

    /// 读取记忆追踪完整内容
    ///
    /// 根据索引中的 date_path + byte_start + byte_length 读取内容
    ///
    /// # 参数
    /// - index: 短期索引
    /// # 返回
    /// - 完整内容字符串
    fn read_memory_content(&self, index: &ShortTermMemoryIndexPo) -> Result<String, AppError>;

    /// 批量追加多个记忆追踪，并批量插入短期索引
    ///
    /// 用于批量聚合记忆，合并多个连续相关记忆追踪为一个短期索引
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - traces: 记忆追踪列表，每个包含 trace + summary + tags
    /// # 返回
    /// - 短期索引列表
    async fn batch_append_memory_traces(
        &self,
        ctx: RequestContext,
        traces: &[(MemoryTrace, String, Vec<String>)],
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<Option<LongTermKnowledgeNodePo>, AppError>;

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
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError>;

    /// 全文检索知识节点
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - query: 搜索关键词
    /// - limit: 最大返回条数
    /// # 返回
    /// - 匹配的节点列表（按相关性排序）
    async fn search_knowledge_nodes(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>, AppError>;

    /// 删除知识节点
    ///
    /// 同时删除相关的引用
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - id: 节点 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<(), AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<Vec<KnowledgeReferencePo>, AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<(), AppError>;

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
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;

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
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;

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
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;

    /// 删除指定关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - relation_id: 关系 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation_id: &str,
    ) -> Result<(), AppError>;

    /// 删除节点的所有关系
    ///
    /// 当删除节点时调用，清理所有相关关系
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - node_id: 节点 ID
    /// # 返回
    /// - 成功返回 Ok(())
    async fn delete_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<(), AppError>;

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
    ) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;
}

// ==================== SQLite 实现 ====================

pub mod sqlite;
pub use self::sqlite::{dao, init, SqliteMemoryDao};



#[cfg(test)]
 mod sqlite_test;
