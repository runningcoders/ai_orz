//! Task Vector DAO implementation
//! 负责任务向量索引的 CRUD 操作，与基础任务数据完全解耦

use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskVectorDao;
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static TASK_VECTOR_DAO: OnceLock<Arc<dyn TaskVectorDao>> = OnceLock::new();

/// 创建一个全新的 Task Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn TaskVectorDao> {
    Arc::new(TaskVectorDaoImpl)
}

/// 获取 Task Vector DAO 单例
pub fn dao() -> Arc<dyn TaskVectorDao> {
    TASK_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = TASK_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================

/// 任务向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct TaskVectorDaoImpl;

#[async_trait]
impl TaskVectorDao for TaskVectorDaoImpl {
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        task_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = ctx.vector_store();
        vector_store.upsert("tasks", task_id, vector_params).await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = ctx.vector_store();
        let results = vector_store.search("tasks", query_vector, top_k).await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Option<VectorRow>> {
        ctx.vector_store()
            .get("tasks", task_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, ctx: RequestContext, task_id: &str) -> Result<()> {
        let vector_store = ctx.vector_store();
        vector_store.delete("tasks", task_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("tasks").await?;
        Ok(())
    }
}
