//! Model Provider 具体实现

use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::domain::finance::{FinanceDomainImpl, ModelProviderManage};
use common::error::{Error, ErrorField, ErrorCode, Result};
use common::enums::{ModelCapability, ModelProviderStatus};
use serde_json::json;

use crate::enrich_ctx;

#[async_trait::async_trait]
impl ModelProviderManage for FinanceDomainImpl {
    async fn create_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.create(ctx, provider).await
    }

    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>> {
        self.model_provider_dal.find_by_id(ctx, id).await
    }

    async fn get_model_provider_with_options(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::model_provider::ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>> {
        self.model_provider_dal.get_model_provider(ctx, id, options).await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>> {
        self.model_provider_dal.query(ctx, query).await
    }

    async fn list_model_providers(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<ModelProvider>> {
        self.model_provider_dal.find_all(ctx).await
    }

    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        if provider.po.capability.is_embedding() && provider.po.status == ModelProviderStatus::Normal {
            if let Some(current) = self.model_provider_dal.find_enabled_embedding_provider(ctx.clone()).await? {
                if current.po.id != provider.po.id {
                    let mut field = ErrorField::new();
                    field.insert("current_provider_id".into(), json!(current.po.id));
                    field.insert("current_provider_name".into(), json!(current.po.name));
                    return Err(Error::new(
                        ErrorCode::EmbeddingProviderSwitchRequired,
                        format!("Another embedding provider '{}' is already enabled", current.po.name)
                    ).with_field(field));
                }
            }
        }

        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.update(ctx, provider).await
    }

    async fn delete_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.delete(ctx, provider).await
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.brain_dal.test_connection(ctx, provider, prompt).await
    }

    async fn switch_embedding_provider(
        &self,
        ctx: RequestContext,
        new_provider_id: &str,
    ) -> Result<Option<ModelProvider>> {
        let new_provider = self.get_model_provider(ctx.clone(), new_provider_id).await?
            .ok_or_else(|| Error::not_found(format!("ModelProvider {} not found", new_provider_id)))?;

        if !new_provider.po.capability.is_embedding() {
            return Err(Error::bad_request("Target provider is not an embedding provider"));
        }

        let current_provider = self.model_provider_dal.find_enabled_embedding_provider(ctx.clone()).await?;

        if let Some(mut current) = current_provider.clone() {
            if current.po.id == new_provider_id {
                return Ok(current_provider);
            }

            current.po.status = ModelProviderStatus::Deleted;
            self.model_provider_dal.update(ctx.clone(), &current).await?;
        }

        let mut new_provider_to_enable = new_provider.clone();
        new_provider_to_enable.po.status = ModelProviderStatus::Normal;
        self.update_model_provider(ctx.clone(), &new_provider_to_enable).await?;

        // 重建所有向量索引
        self.rebuild_all_vector_indexes(ctx).await?;

        Ok(current_provider)
    }
}

impl FinanceDomainImpl {
    /// 重建所有向量索引（切换 Embedding Provider 后调用）
    async fn rebuild_all_vector_indexes(&self, ctx: RequestContext) -> Result<()> {
        use crate::service::dal;

        log_info!(&ctx, "rebuild_vectors", "开始重建所有向量索引");

        // 依次调用各业务 DAL 的 rebuild_vectors
        if let Err(e) = dal::agent::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Agent 向量重建失败");
        }
        if let Err(e) = dal::memory::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Memory 向量重建失败");
        }
        if let Err(e) = dal::skill::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Skill 向量重建失败");
        }
        if let Err(e) = dal::task::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Task 向量重建失败");
        }
        if let Err(e) = dal::project::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Project 向量重建失败");
        }
        if let Err(e) = dal::message::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Message 向量重建失败");
        }
        if let Err(e) = dal::tool::dal().rebuild_vectors(ctx.clone()).await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "Tool 向量重建失败");
        }

        log_info!(&ctx, "rebuild_vectors", "所有向量索引重建完成");
        Ok(())
    }
}
