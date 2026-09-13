//! Skill Vector DAO implementation
//! 负责技能向量索引的 CRUD 操作，与基础技能数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
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
        _ctx: RequestContext,
        skill_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("skills", skill_id, vector_params)
            .await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &SkillQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("skills", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        _ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("skills", skill_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, _ctx: RequestContext, skill_id: &str) -> Result<()> {
        _ctx.vector_store()
            .delete("skills", skill_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("skills").await?;
        Ok(())
    }
}

// ==================== SkillQuery → VectorFilter 白名单转译 ====================

/// SkillQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：category（与 payload.entity_type 同构）。
/// 未覆盖字段（status / exclude_status / author_id / parent_skill_id / has_parent /
/// tags / ids / keyword / pagination）由回业务表过滤兜底：
/// - payload 未落 status（SkillPo::vector_payload 仅落 tags + entity_type）
/// - tags 落库为 JSON 数组字符串，VectorFilter 无数组成员判断语义，无法下推
pub(crate) fn translate_filters(query: &SkillQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(category) = &query.category {
        conditions.push(VectorFilter::Eq(
            VectorField::EntityType,
            FilterValue::Str(category.clone()),
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

    #[test]
    fn test_translate_category_only() {
        let q = SkillQuery {
            category: Some("coding".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::EntityType, FilterValue::Str("coding".into()))
        );
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&SkillQuery::default()).is_none());
        // 非白名单字段不触发转译（status / tags 均无 payload 映射或无成员判断语义）
        let q = SkillQuery {
            status: Some(common::enums::SkillStatus::Published),
            tags: Some(vec!["math".into()]),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
