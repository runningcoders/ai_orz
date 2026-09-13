//! Agent Vector DAO implementation
//! 负责 Agent 向量索引的 CRUD 操作，与基础 Agent 数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static AGENT_VECTOR_DAO: OnceLock<Arc<dyn AgentVectorDao>> = OnceLock::new();

/// 创建一个全新的 Agent Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn AgentVectorDao> {
    Arc::new(AgentVectorDaoImpl)
}

/// 获取 Agent Vector DAO 单例
pub fn dao() -> Arc<dyn AgentVectorDao> {
    AGENT_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = AGENT_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================
/// Agent 向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct AgentVectorDaoImpl;

#[async_trait]
impl AgentVectorDao for AgentVectorDaoImpl {
    async fn upsert_vector(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("agents", agent_id, vector_params)
            .await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &AgentQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("agents", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("agents", agent_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, _ctx: RequestContext, agent_id: &str) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.delete("agents", agent_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("agents").await?;
        Ok(())
    }
}

// ==================== AgentQuery → VectorFilter 白名单转译 ====================

/// AgentQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：status / exclude_status（补集 Any）。
/// 未覆盖字段（ids / keyword / created_by / model_provider_id / roles / runtime_state /
/// pagination）由回业务表过滤兜底。
///
/// AgentPo 的 payload 仅落库 status，故只有 status 维度可下推；exclude_status
/// 是 search() 默认注入的软删除排除条件，软删除 Agent 的向量行仍在库中，
/// 不下推会持续稀释 Top-K。
pub(crate) fn translate_filters(query: &AgentQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(status) = query.status {
        // payload.status 以 i32 字符串落库（与 AgentPo::vector_payload 同构）
        conditions.push(VectorFilter::Eq(
            VectorField::Status,
            FilterValue::Str(status.to_i32().to_string()),
        ));
    }
    if let Some(excluded) = query.exclude_status {
        // NOT 语义用补集 Any 表达（VectorFilter 无 Not 变体，保持四后端实现简单）
        let complement: Vec<VectorFilter> = common::enums::AgentStatus::all()
            .iter()
            .filter(|s| **s != excluded)
            .map(|s| {
                VectorFilter::Eq(
                    VectorField::Status,
                    FilterValue::Str(s.to_i32().to_string()),
                )
            })
            .collect();
        if !complement.is_empty() {
            conditions.push(VectorFilter::Any(complement));
        }
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
    use common::enums::AgentStatus;

    #[test]
    fn test_translate_status_only() {
        let q = AgentQuery {
            status: Some(AgentStatus::Onboarded),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::Status, FilterValue::Str("3".into()))
        );
    }

    #[test]
    fn test_translate_exclude_status_complement() {
        // search() 默认注入 exclude_status=Deleted → 补集 = 全部非 Deleted 状态
        let q = AgentQuery {
            exclude_status: Some(AgentStatus::Deleted),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::Any(options) => {
                assert_eq!(options.len(), AgentStatus::all().len() - 1);
                assert!(!options.contains(&VectorFilter::Eq(
                    VectorField::Status,
                    FilterValue::Str("0".into())
                )));
            }
            other => panic!("expect Any, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_combined_all() {
        let q = AgentQuery {
            status: Some(AgentStatus::Onboarded),
            exclude_status: Some(AgentStatus::Deleted),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::All(fs) => assert_eq!(fs.len(), 2),
            other => panic!("expect All, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&AgentQuery::default()).is_none());
        // 非白名单字段不触发转译
        let q = AgentQuery {
            created_by: Some("user-1".into()),
            roles: Some(vec!["worker".into()]),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
