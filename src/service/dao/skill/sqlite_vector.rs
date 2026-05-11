//! SQLite implementation of Skill Vector DAO
//! 负责技能向量索引的 CRUD 操作，与基础技能数据完全解耦

use async_trait::async_trait;
use crate::error::AppError;
use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillVectorDao;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static SKILL_VECTOR_DAO: OnceLock<Arc<dyn SkillVectorDao>> = OnceLock::new();

/// 创建一个全新的 Skill Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn SkillVectorDao> {
    Arc::new(SkillVectorDaoSqliteImpl)
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

#[derive(Debug, Clone)]
struct SkillVectorDaoSqliteImpl;

#[async_trait]
impl SkillVectorDao for SkillVectorDaoSqliteImpl {
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
            &vector_params.vector,
            &vector_params.content_hash,
            &vector_params.embedding_model,
            vector_params.expire_at,
        ).await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<(String, f32)>, AppError> {
        let vector_store = ctx.vector_store();
        let results = vector_store.search(
            "skills",
            query_vector,
            top_k,
        ).await?;
        Ok(results)
    }

    async fn get_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError> {
        ctx.vector_store()
            .get_content_hash("skills", skill_id)
            .await
            .map_err(|e| AppError::Internal(format!("Vector store error: {}", e)))
    }
}
