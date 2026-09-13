//! Message Vector DAO implementation
//! 负责消息向量索引的 CRUD 操作，与基础消息数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::message::{MessageQuery, MessageVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static MESSAGE_VECTOR_DAO: OnceLock<Arc<dyn MessageVectorDao>> = OnceLock::new();

/// 创建一个全新的 Message Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn MessageVectorDao> {
    Arc::new(MessageVectorDaoImpl)
}

/// 获取 Message Vector DAO 单例
pub fn dao() -> Arc<dyn MessageVectorDao> {
    MESSAGE_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = MESSAGE_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================
/// 消息向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct MessageVectorDaoImpl;

#[async_trait]
impl MessageVectorDao for MessageVectorDaoImpl {
    async fn upsert_vector(
        &self,
        _ctx: RequestContext,
        message_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store
            .upsert("messages", message_id, vector_params)
            .await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &MessageQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("messages", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        _ctx: RequestContext,
        message_id: &str,
    ) -> Result<Option<VectorRow>> {
        _ctx.vector_store()
            .get("messages", message_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, _ctx: RequestContext, message_id: &str) -> Result<()> {
        let vector_store = _ctx.vector_store();
        vector_store.delete("messages", message_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("messages").await?;
        Ok(())
    }
}

// ==================== MessageQuery → VectorFilter 白名单转译 ====================

/// MessageQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：organization_id / task_id / project_id / from_id / to_id / status_in。
/// 未覆盖字段（id / ids / reply_to_id / root_id / to_role / message_type / keyword /
/// limit / offset / order_by）由回业务表过滤兜底。
pub(crate) fn translate_filters(query: &MessageQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(org_id) = &query.organization_id {
        conditions.push(VectorFilter::Eq(
            VectorField::OrgId,
            FilterValue::Str(org_id.clone()),
        ));
    }
    if let Some(task_id) = &query.task_id {
        conditions.push(VectorFilter::Eq(
            VectorField::TaskId,
            FilterValue::Str(task_id.clone()),
        ));
    }
    if let Some(project_id) = &query.project_id {
        conditions.push(VectorFilter::Eq(
            VectorField::ProjectId,
            FilterValue::Str(project_id.clone()),
        ));
    }
    if let Some(from_id) = &query.from_id {
        conditions.push(VectorFilter::Eq(
            VectorField::FromId,
            FilterValue::Str(from_id.clone()),
        ));
    }
    if let Some(to_id) = &query.to_id {
        conditions.push(VectorFilter::Eq(
            VectorField::ToId,
            FilterValue::Str(to_id.clone()),
        ));
    }
    if let Some(statuses) = &query.status_in
        && !statuses.is_empty()
    {
        // payload.status 以 i32 字符串落库（与 MessagePo::vector_payload 同构）
        let options: Vec<VectorFilter> = statuses
            .iter()
            .map(|s| {
                VectorFilter::Eq(
                    VectorField::Status,
                    FilterValue::Str(s.to_i32().to_string()),
                )
            })
            .collect();
        conditions.push(VectorFilter::Any(options));
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
    use common::enums::MessageStatus;

    #[test]
    fn test_translate_to_id_only() {
        let q = MessageQuery {
            to_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::ToId, FilterValue::Str("agent-1".into()))
        );
    }

    #[test]
    fn test_translate_org_isolation() {
        let q = MessageQuery {
            organization_id: Some("org-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::OrgId, FilterValue::Str("org-1".into()))
        );
    }

    #[test]
    fn test_translate_status_in_any() {
        let q = MessageQuery {
            status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processed]),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::Any(options) => {
                assert_eq!(options.len(), 2);
                assert!(options.contains(&VectorFilter::Eq(
                    VectorField::Status,
                    FilterValue::Str(MessageStatus::Pending.to_i32().to_string())
                )));
            }
            other => panic!("expect Any, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_combined_all() {
        let q = MessageQuery {
            organization_id: Some("org-1".into()),
            task_id: Some("task-9".into()),
            from_id: Some("user-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::All(fs) => assert_eq!(fs.len(), 3),
            other => panic!("expect All, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&MessageQuery::default()).is_none());
        // 非白名单字段不触发转译
        let q = MessageQuery {
            message_type: Some(common::enums::MessageType::Text),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
