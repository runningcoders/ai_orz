//! Tool Vector DAO implementation
//! 负责工具向量索引的 CRUD 操作，与基础工具数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::tool::{ToolQuery, ToolVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static TOOL_VECTOR_DAO: OnceLock<Arc<dyn ToolVectorDao>> = OnceLock::new();

/// 创建一个全新的 Tool Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn ToolVectorDao> {
    Arc::new(ToolVectorDaoImpl)
}

/// 获取 Tool Vector DAO 单例
pub fn dao() -> Arc<dyn ToolVectorDao> {
    TOOL_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = TOOL_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================
/// 工具向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct ToolVectorDaoImpl;

#[async_trait]
impl ToolVectorDao for ToolVectorDaoImpl {
    async fn upsert_vector(
        &self,
        _ctx: RequestContext,
        tool_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.upsert("tools", tool_id, vector_params).await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &ToolQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("tools", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        _ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("tools", tool_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, _ctx: RequestContext, tool_id: &str) -> Result<()> {
        _ctx.vector_store()
            .delete("tools", tool_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("tools").await?;
        Ok(())
    }
}

// ==================== ToolQuery → VectorFilter 白名单转译 ====================

/// ToolQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：status / exclude_status（补集 Any）。
/// 未覆盖字段（agent_id / ids / keyword / tags / protocol / mcp_server_id /
/// enabled_only / pagination）由回业务表过滤兜底：
/// - payload 未落 agent_id（工具是全局实体，Agent 绑定关系在业务表）
/// - tags 落库为 JSON 数组字符串，VectorFilter 无数组成员判断语义，无法下推
/// - enabled_only 在业务表与 status 语义并非严格等价，保守不转译
pub(crate) fn translate_filters(query: &ToolQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(status) = query.status {
        // payload.status 以 i32 字符串落库（与 ToolPo::vector_payload 同构）
        conditions.push(VectorFilter::Eq(
            VectorField::Status,
            FilterValue::Str(status.to_i32().to_string()),
        ));
    }
    if let Some(excluded) = query.exclude_status {
        // NOT 语义用补集 Any 表达（VectorFilter 无 Not 变体，保持四后端实现简单）
        let complement: Vec<VectorFilter> = common::enums::ToolStatus::all()
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
    use common::enums::ToolStatus;

    #[test]
    fn test_translate_status_only() {
        let q = ToolQuery {
            status: Some(ToolStatus::Enabled),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::Status, FilterValue::Str("1".into()))
        );
    }

    #[test]
    fn test_translate_exclude_status_complement() {
        let q = ToolQuery {
            exclude_status: Some(ToolStatus::Disabled),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::Any(options) => {
                assert_eq!(options.len(), ToolStatus::all().len() - 1);
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
        let q = ToolQuery {
            status: Some(ToolStatus::Enabled),
            exclude_status: Some(ToolStatus::Disabled),
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
        assert!(translate_filters(&ToolQuery::default()).is_none());
        // 非白名单字段不触发转译
        let q = ToolQuery {
            agent_id: Some("agent-1".into()),
            tags: Some(vec!["math".into()]),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
