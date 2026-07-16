//! Project Vector DAO implementation
//! 负责项目向量索引的 CRUD 操作，与基础项目数据完全解耦
//!
//! 基于 ctx.vector_store() 通用 VectorStore trait，不绑定具体数据库。
//! collection 名称为 "projects"，对应 vss_projects 向量表。

use common::error::{err, Result};
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectVectorDao;
use async_trait::async_trait;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static PROJECT_VECTOR_DAO: OnceLock<Arc<dyn ProjectVectorDao>> = OnceLock::new();

/// 创建一个全新的 Project Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn ProjectVectorDao> {
    Arc::new(ProjectVectorDaoImpl)
}

/// 获取 Project Vector DAO 单例
pub fn dao() -> Arc<dyn ProjectVectorDao> {
    PROJECT_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = PROJECT_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================

/// 项目向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct ProjectVectorDaoImpl;

#[async_trait]
impl ProjectVectorDao for ProjectVectorDaoImpl {
    async fn upsert_vector(
        &self,
        _ctx: RequestContext,
        project_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("projects", project_id, vector_params)
            .await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let results = vector_store
            .search("projects", query_vector, top_k)
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        _ctx: RequestContext,
        project_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("projects", project_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(
        &self,
        _ctx: RequestContext,
        project_id: &str,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.delete("projects", project_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("projects").await?;
        Ok(())
    }
}
