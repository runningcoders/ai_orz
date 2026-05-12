//! Memory Vector DAO implementation
//! 负责记忆向量索引的 CRUD 操作，与基础记忆数据完全解耦

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryVectorDao;
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
        ctx: RequestContext,
        memory_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError> {
        let vector_store = ctx.vector_store();
        vector_store.upsert(
            "memory:short_term",
            memory_id,
            vector_params,
        ).await?;
        Ok(())
    }

    /// 索引长期知识节点向量（node_description + summary 拼接）
    async fn upsert_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError> {
        let vector_store = ctx.vector_store();
        vector_store.upsert(
            "memory:knowledge_node",
            knowledge_id,
            vector_params,
        ).await?;
        Ok(())
    }

    /// 语义搜索短期记忆
    async fn search_short_term_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>, AppError> {
        let vector_store = ctx.vector_store();
        let results = vector_store.search(
            "memory:short_term",
            query_vector,
            top_k,
        ).await?;
        Ok(results)
    }

    /// 语义搜索长期知识节点
    async fn search_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>, AppError> {
        let vector_store = ctx.vector_store();
        let results = vector_store.search(
            "memory:knowledge_node",
            query_vector,
            top_k,
        ).await?;
        Ok(results)
    }

    /// 获取指定短期记忆的完整向量行数据
    async fn get_short_term_vector_row(
        &self,
        ctx: RequestContext,
        memory_id: &str,
    ) -> Result<Option<VectorRow>, AppError> {
        ctx.vector_store()
            .get("memory:short_term", memory_id)
            .await
            .map_err(|e| AppError::Internal(format!("Vector store error: {e}")))
    }

    /// 获取指定知识节点的完整向量行数据
    async fn get_knowledge_node_vector_row(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Option<VectorRow>, AppError> {
        ctx.vector_store()
            .get("memory:knowledge_node", knowledge_id)
            .await
            .map_err(|e| AppError::Internal(format!("Vector store error: {e}")))
    }

    /// 删除短期记忆的向量索引
    async fn delete_short_term_vector(
        &self,
        ctx: RequestContext,
        memory_id: &str,
    ) -> Result<(), AppError> {
        let vector_store = ctx.vector_store();
        vector_store.delete("memory:short_term", memory_id).await?;
        Ok(())
    }

    /// 删除知识节点的向量索引
    async fn delete_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<(), AppError> {
        let vector_store = ctx.vector_store();
        vector_store.delete("memory:knowledge_node", knowledge_id).await?;
        Ok(())
    }
}
