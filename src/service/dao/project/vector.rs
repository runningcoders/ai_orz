//! Project Vector DAO implementation
//! 负责项目向量索引的 CRUD 操作，与基础项目数据完全解耦
//!
//! 基于 ctx.vector_store() 通用 VectorStore trait，不绑定具体数据库。
//! collection 名称为 "projects"，对应 vss_projects 向量表。

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectQuery, ProjectVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
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
        filters: &ProjectQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("projects", query_vector, top_k, filter.as_ref())
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

    async fn delete_vector(&self, _ctx: RequestContext, project_id: &str) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.delete("projects", project_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("projects").await?;
        Ok(())
    }
}

// ==================== ProjectQuery → VectorFilter 白名单转译 ====================

/// ProjectQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：owner_agent_id（与 payload.agent_id 同构）。
/// 未覆盖字段（root_user_id / status_in / ids / keyword / pagination）由回业务表
/// 过滤兜底；payload 未落 status，故 status_in 不下推。
pub(crate) fn translate_filters(query: &ProjectQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(owner_agent_id) = &query.owner_agent_id {
        conditions.push(VectorFilter::Eq(
            VectorField::AgentId,
            FilterValue::Str(owner_agent_id.clone()),
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
    fn test_translate_owner_agent_only() {
        let q = ProjectQuery {
            owner_agent_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()))
        );
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&ProjectQuery::default()).is_none());
        // 非白名单字段不触发转译（root_user_id / status_in 均不在 payload 白名单内）
        let q = ProjectQuery {
            root_user_id: Some("user-1".into()),
            status_in: Some(vec![common::enums::ProjectStatus::Active]),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
