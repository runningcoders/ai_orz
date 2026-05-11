//! Skill Vector DAO implementation
//! 负责技能向量索引的 CRUD 操作，与基础技能数据完全解耦

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillVectorDao;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static SKILL_VECTOR_DAO: OnceLock<Arc<dyn SkillVectorDao>> = OnceLock::new();

/// 创建一个全新的 Skill Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn SkillVectorDao> {
    Arc::new(SkillVectorDaoImpl)
}

/// 获取 Skill Vector DAO 单例
pub fn dao() -> Arc<dyn SkillVectorDao> {
    SKILL_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = SKILL_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================
/// 技能向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct SkillVectorDaoImpl;

#[async_trait]
impl SkillVectorDao for SkillVectorDaoImpl {
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError> {
        let vector_store = ctx.vector_store();
        vector_store.upsert(
            "skills",
            skill_id,
            vector_params,
        ).await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>, AppError> {
        let vector_store = ctx.vector_store();
        let results = vector_store.search(
            "skills",
            query_vector,
            top_k,
        ).await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<VectorRow>, AppError> {
        ctx.vector_store()
            .get("skills", skill_id)
            .await
            .map_err(|e| AppError::Internal(format!("Vector store error: {}", e)))
    }
}
