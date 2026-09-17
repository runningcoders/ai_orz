//! Memory Vector DAO implementation
//! 负责记忆向量索引的 CRUD 操作，与基础记忆数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemoryVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static MEMORY_VECTOR_DAO: OnceLock<Arc<dyn MemoryVectorDao>> = OnceLock::new();

/// 创建一个全新的 Memory Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn MemoryVectorDao> {
    Arc::new(MemoryVectorDaoImpl)
}

/// 获取 Memory Vector DAO 单例
pub fn dao() -> Arc<dyn MemoryVectorDao> {
    MEMORY_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MEMORY_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================
/// 记忆向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct MemoryVectorDaoImpl;

#[async_trait]
impl MemoryVectorDao for MemoryVectorDaoImpl {
    /// 索引短期记忆向量（summary 字段）
    async fn upsert_short_term_vector(
        &self,
        _ctx: RequestContext,
        _memory_id: &str,
        _vector_params: &VectorIndexParams,
    ) -> Result<()> {
        // already handled by ? conversion
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("memory:short_term", _memory_id, _vector_params)
            .await?;
        Ok(())
    }

    /// 索引长期知识节点向量（node_description + summary 拼接）
    async fn upsert_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        _knowledge_id: &str,
        _vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("memory:knowledge_node", _knowledge_id, _vector_params)
            .await?;
        Ok(())
    }

    /// 语义搜索短期记忆（业务过滤在 DAO 内转译为向量谓词下推）
    async fn search_short_term_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("memory:short_term", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    /// 语义搜索长期知识节点（业务过滤在 DAO 内转译为向量谓词下推）
    async fn search_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search(
                "memory:knowledge_node",
                query_vector,
                top_k,
                filter.as_ref(),
            )
            .await?;
        Ok(results)
    }

    /// 获取指定短期记忆的完整向量行数据
    async fn get_short_term_vector_row(
        &self,
        _ctx: RequestContext,
        memory_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("memory:short_term", memory_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    /// 获取指定知识节点的完整向量行数据
    async fn get_knowledge_node_vector_row(
        &self,
        _ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("memory:knowledge_node", knowledge_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    /// 删除短期记忆的向量索引
    async fn delete_short_term_vector(&self, _ctx: RequestContext, memory_id: &str) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.delete("memory:short_term", memory_id).await?;
        Ok(())
    }

    /// 删除知识节点的向量索引
    async fn delete_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .delete("memory:knowledge_node", knowledge_id)
            .await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.clear_collection("memory:short_term").await?;
        vector_store
            .clear_collection("memory:knowledge_node")
            .await?;
        Ok(())
    }
}

// ==================== MemoryQuery → VectorFilter 白名单转译 ====================

/// MemoryQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：agent_id（**显式归属筛选**）/ task_id / node_type。
/// 未覆盖字段（status / exclude_status / tags / keyword / ids / limit / order）由回业务表过滤兜底。
///
/// ⚠️ 知识节点的可见性**不在这里收窄**：蜂巢模式下所有 Agent 共享全部知识节点，
/// 向量检索同样不能被归属门槛挡住（否则「全局 = 只看 published」会让语义检索大面积漏召回）。
/// `agent_id` 只在调用方明确要求「只看某个 Agent 的记忆」时才下推，空串与 `None` 同义。
pub(crate) fn translate_filters(query: &MemoryQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(agent_id) = query.agent_id.clone().filter(|s| !s.is_empty()) {
        conditions.push(VectorFilter::Eq(
            VectorField::AgentId,
            FilterValue::Str(agent_id),
        ));
    }
    if let Some(task_id) = &query.task_id {
        conditions.push(VectorFilter::Eq(
            VectorField::TaskId,
            FilterValue::Str(task_id.clone()),
        ));
    }
    if let Some(node_type) = &query.node_type {
        conditions.push(VectorFilter::Eq(
            VectorField::EntityType,
            FilterValue::Str(node_type.clone()),
        ));
    }

    match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(VectorFilter::All(conditions)),
    }
}

#[cfg(test)]
mod translate_tests {
    use super::*;
    use crate::models::vector::{FilterValue, VectorField, VectorFilter};

    #[test]
    fn test_translate_agent_only() {
        let q = MemoryQuery {
            agent_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()))
        );
    }

    /// 空串与 `None` 同义：都表示「不筛选归属」（蜂巢全域）。
    /// 若把空串当值下推，向量检索会变成 `agent_id = ''` → 恒空，直接漏召回。
    #[test]
    fn test_translate_empty_agent_means_global() {
        for agent in [None, Some(String::new())] {
            let q = MemoryQuery {
                agent_id: agent.clone(),
                ..Default::default()
            };
            assert!(
                translate_filters(&q).is_none(),
                "空归属不该下推任何谓词: {agent:?}"
            );
        }
    }

    /// 归属筛选是**纯 Eq**，不再叠加 `is_published` 的 OR 分支：
    /// 知识节点蜂巢共享，published 只是重要性标记，不是可见性门槛。
    #[test]
    fn test_translate_agent_is_plain_eq_without_published_or() {
        let q = MemoryQuery {
            agent_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            "不应再出现 Any[agent, is_published] 这种共享可见性谓词"
        );
    }

    #[test]
    fn test_translate_combined_all() {
        let q = MemoryQuery {
            agent_id: Some("agent-1".into()),
            task_id: Some("task-9".into()),
            node_type: Some("concept".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        // All[Eq(agent), Eq(task), Eq(entity_type)]
        match f {
            VectorFilter::All(fs) => assert_eq!(fs.len(), 3),
            other => panic!("expect All, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&MemoryQuery::default()).is_none());
    }
}
