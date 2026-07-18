//! Memory Vector DAO implementation
//! 负责记忆向量索引的 CRUD 操作，与基础记忆数据完全解耦

use common::error::{err, Result};
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryVectorDao;
use async_trait::async_trait;
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

    /// 语义搜索短期记忆
    async fn search_short_term_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let results = vector_store
            .search("memory:short_term", query_vector, top_k)
            .await?;
        Ok(results)
    }

    /// 语义搜索长期知识节点
    async fn search_knowledge_node_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let results = vector_store
            .search("memory:knowledge_node", query_vector, top_k)
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
    async fn delete_short_term_vector(
        &self,
        _ctx: RequestContext,
        memory_id: &str,
    ) -> Result<()> {
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
        vector_store.clear_collection("memory:knowledge_node").await?;
        Ok(())
    }
}