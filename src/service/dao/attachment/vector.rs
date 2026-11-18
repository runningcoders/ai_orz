//! Attachment Vector DAO implementation
//! 负责附件向量索引的 CRUD 操作，与基础附件数据完全解耦。
//! collection 名为 "attachments"，向量化文本 = 文件名 + AI 摘要 + 用途 + purpose。

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::attachment::{AttachmentQuery, AttachmentVectorDao};
use async_trait::async_trait;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static ATTACHMENT_VECTOR_DAO: OnceLock<Arc<dyn AttachmentVectorDao>> = OnceLock::new();

/// 创建一个全新的 Attachment Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn AttachmentVectorDao> {
    Arc::new(AttachmentVectorDaoImpl)
}

/// 获取 Attachment Vector DAO 单例
pub fn dao() -> Arc<dyn AttachmentVectorDao> {
    ATTACHMENT_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ATTACHMENT_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================

#[derive(Debug, Clone)]
pub struct AttachmentVectorDaoImpl;

#[async_trait]
impl AttachmentVectorDao for AttachmentVectorDaoImpl {
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        attachment_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        ctx.vector_store()
            .upsert("attachments", attachment_id, vector_params)
            .await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &AttachmentQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let filter = translate_filters(filters);
        ctx.vector_store()
            .search("attachments", query_vector, top_k, filter.as_ref())
            .await
    }

    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        attachment_id: &str,
    ) -> Result<Option<VectorRow>> {
        ctx.vector_store()
            .get("attachments", attachment_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, ctx: RequestContext, attachment_id: &str) -> Result<()> {
        ctx.vector_store()
            .delete("attachments", attachment_id)
            .await?;
        Ok(())
    }

    async fn clear_collection(&self, ctx: RequestContext) -> Result<()> {
        ctx.vector_store().clear_collection("attachments").await?;
        Ok(())
    }
}

// ==================== AttachmentQuery → VectorFilter 白名单转译 ====================

/// 白名单：root_user_id → AgentId（归属隔离，越权红线）、file_type → EntityType。
/// ⚠️ purpose 不进向量谓词：payload.entity_type 存的是 FileType 的 Debug 串
/// （Document/Image/…），与 purpose 值域（skill/message/…）无交集，强行下推会让
/// 向量语义路永久零命中；purpose 过滤由回填阶段的业务表 `purpose` 列兜底。
/// 其余字段（ids / keyword / pagination）同样由回业务表过滤兜底。
pub(crate) fn translate_filters(query: &AttachmentQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(root_user_id) = &query.root_user_id {
        conditions.push(VectorFilter::Eq(
            VectorField::AgentId,
            FilterValue::Str(root_user_id.clone()),
        ));
    }
    // 只有 file_type 落 entity_type，值域两端都是 FileType 的 Debug 串，匹配成立。
    // ⚠️ 不新增 bincode 字段（会让存量向量解码失败静默降级），因此 purpose 没有独立列，
    // 绝不能复用 entity_type——值域不同会导致向量路对任何 purpose 都零命中。
    if let Some(file_type) = &query.file_type {
        conditions.push(VectorFilter::Eq(
            VectorField::EntityType,
            FilterValue::Str(format!("{file_type:?}")),
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
    fn translate_owner_to_agent_id() {
        let q = AttachmentQuery {
            root_user_id: Some("user-1".into()),
            ..Default::default()
        };
        assert_eq!(
            translate_filters(&q),
            Some(VectorFilter::Eq(
                VectorField::AgentId,
                FilterValue::Str("user-1".into())
            ))
        );
    }

    #[test]
    fn translate_none_for_empty_query() {
        assert!(translate_filters(&AttachmentQuery::default()).is_none());
    }

    #[test]
    fn purpose_is_not_translated_into_vector_predicate() {
        // purpose 值域（skill/message/…）与 payload.entity_type（FileType Debug 串）无交集。
        // 下推会让向量路对任何 purpose 都零命中，故只保留归属条件，purpose 交给回表过滤。
        let q = AttachmentQuery {
            root_user_id: Some("user-1".into()),
            purpose: Some("skill".into()),
            ..Default::default()
        };
        assert_eq!(
            translate_filters(&q),
            Some(VectorFilter::Eq(
                VectorField::AgentId,
                FilterValue::Str("user-1".into())
            ))
        );
    }

    #[test]
    fn file_type_is_translated_into_entity_type() {
        let q = AttachmentQuery {
            file_type: Some(common::enums::FileType::Image),
            ..Default::default()
        };
        assert_eq!(
            translate_filters(&q),
            Some(VectorFilter::Eq(
                VectorField::EntityType,
                FilterValue::Str("Image".into())
            ))
        );
    }
}
