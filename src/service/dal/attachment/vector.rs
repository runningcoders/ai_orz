//! Attachment 向量索引：写库钩子（三态重索引）与全量重建。
//!
//! 全部为增强路：依赖缺失 / embedding 失败一律 warn 降级，绝不阻塞附件主链路。

use super::AttachmentDalImpl;
use crate::models::attachment::AttachmentPo;
use crate::models::vector::{ReindexDecision, Vectorizable};
use crate::pkg::RequestContext;
use crate::service::dal::VECTOR_REBUILD_PAGE_SIZE;
use common::enums::VectorRebuildMode;
use common::error::Result;

impl AttachmentDalImpl {
    /// 写库钩子：为单个附件构建/刷新向量索引（三态：Skip / PayloadOnly / FullReindex）。
    /// 任何一步失败只 warn，不影响调用方的元数据写入结果。
    pub(super) async fn reindex_vector(&self, ctx: &RequestContext, po: &AttachmentPo) {
        let (Some(vector_dao), Some(cortex_dao), Some(model_provider_dao)) = (
            &self.attachment_vector_dao,
            &self.cortex_dao,
            &self.model_provider_dao,
        ) else {
            return; // 向量依赖未注入（测试 / 未配置）：静默跳过
        };

        let provider = match model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await
        {
            Ok(Some(provider)) => provider,
            Ok(None) => return, // 无默认 embedding provider：跳过
            Err(e) => {
                log_warn!(ctx, "vector_index", error = ?e, "读取 Embedding Provider 失败，附件向量跳过");
                return;
            }
        };

        // 三态：取旧行比对双 hash（文本 hash + payload hash）
        if let Ok(Some(row)) = vector_dao.get_vector_row(ctx.clone(), &po.id).await {
            match po.reindex_decision(&row.meta.content_hash, &row.meta.payload_hash) {
                ReindexDecision::Skip => return,
                ReindexDecision::PayloadOnly => {
                    if let Err(e) = ctx
                        .vector_store()
                        .update_payload("attachments", &po.id, &po.vector_payload())
                        .await
                    {
                        log_warn!(ctx, "vector_index", attachment_id = %po.id, error = ?e, "附件向量 payload 刷新失败，已降级");
                    }
                    return;
                }
                ReindexDecision::FullReindex => {}
            }
        }

        match cortex_dao.embed_entity(ctx.clone(), &provider, po).await {
            Ok(params) => {
                if let Err(e) = vector_dao.upsert_vector(ctx.clone(), &po.id, &params).await {
                    log_warn!(ctx, "vector_index", attachment_id = %po.id, error = ?e, "附件向量索引写入失败，已降级");
                }
            }
            Err(e) => {
                log_warn!(ctx, "vector_index", attachment_id = %po.id, error = ?e, "附件向量化失败，已降级");
            }
        }
    }

    /// 全量重建附件向量索引（供 RebuildVectorsTask 调用）。
    ///
    /// 三档 mode：当前附件量小且无版本口径，统一清空全量重建（与 message 同口径）；
    /// 保留参数以对接统一的重建框架。
    pub(super) async fn rebuild_vectors_impl(
        &self,
        ctx: RequestContext,
        progress: &crate::pkg::background_task::TaskProgressCounter,
        _mode: VectorRebuildMode,
    ) -> Result<()> {
        let (Some(vector_dao), Some(cortex_dao), Some(model_provider_dao)) = (
            &self.attachment_vector_dao,
            &self.cortex_dao,
            &self.model_provider_dao,
        ) else {
            return Ok(()); // 向量依赖未注入：跳过
        };
        let Some(provider) = model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            return Ok(());
        };

        vector_dao.clear_collection(ctx.clone()).await?;

        let mut offset = 0usize;
        loop {
            let page = self
                .attachment_dao
                .query(
                    ctx.clone(),
                    crate::service::dao::attachment::AttachmentQuery {
                        pagination: common::api::PaginationParams {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                        },
                        ..Default::default()
                    },
                )
                .await?;
            let fetched = page.items.len();
            if fetched == 0 {
                break;
            }
            for po in &page.items {
                match cortex_dao.embed_entity(ctx.clone(), &provider, po).await {
                    Ok(params) => {
                        if let Err(e) = vector_dao.upsert_vector(ctx.clone(), &po.id, &params).await
                        {
                            log_warn!(ctx, "vector_index", attachment_id = %po.id, error = ?e, "附件向量重建写入失败，已降级");
                        }
                    }
                    Err(e) => {
                        log_warn!(ctx, "vector_index", attachment_id = %po.id, error = ?e, "附件向量重建化失败，已降级");
                    }
                }
            }
            offset += fetched;
            progress.advance(fetched);
            if fetched < VECTOR_REBUILD_PAGE_SIZE {
                break;
            }
        }
        Ok(())
    }
}
