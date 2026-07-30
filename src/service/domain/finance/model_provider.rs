//! Model Provider 具体实现

use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::domain::finance::{FinanceDomainImpl, ModelProviderManage};
use common::enums::ModelProviderStatus;
use common::error::{Error, ErrorCode, ErrorField, Result};
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
        self.model_provider_dal
            .get_model_provider(ctx, id, options)
            .await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProvider>> {
        self.model_provider_dal.query(ctx, query).await
    }

    async fn list_model_providers(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>> {
        self.model_provider_dal.find_all(ctx).await
    }

    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        if provider.po.capability.is_embedding()
            && provider.po.status == ModelProviderStatus::Normal
            && let Some(current) = self
                .model_provider_dal
                .find_enabled_embedding_provider(ctx.clone())
                .await?
            && current.po.id != provider.po.id
        {
            let mut field = ErrorField::new();
            field.insert("current_provider_id".into(), json!(current.po.id));
            field.insert("current_provider_name".into(), json!(current.po.name));
            return Err(Error::new(
                ErrorCode::EmbeddingProviderSwitchRequired,
                format!(
                    "Another embedding provider '{}' is already enabled",
                    current.po.name
                ),
            )
            .with_field(field));
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
        let new_provider = self
            .get_model_provider(ctx.clone(), new_provider_id)
            .await?
            .ok_or_else(|| {
                Error::not_found(format!("ModelProvider {} not found", new_provider_id))
            })?;

        if !new_provider.po.capability.is_embedding() {
            return Err(Error::bad_request(
                "Target provider is not an embedding provider",
            ));
        }

        let current_provider = self
            .model_provider_dal
            .find_enabled_embedding_provider(ctx.clone())
            .await?;

        if let Some(ref current) = current_provider
            && current.po.id == new_provider_id
        {
            // 同一 provider，无需切换
            return Ok(current_provider);
        }

        if let Some(mut current) = current_provider.clone() {
            current.po.status = ModelProviderStatus::Deleted;
            self.model_provider_dal
                .update(ctx.clone(), &current)
                .await?;
        }

        let mut new_provider_to_enable = new_provider.clone();
        new_provider_to_enable.po.status = ModelProviderStatus::Normal;
        self.update_model_provider(ctx.clone(), &new_provider_to_enable)
            .await?;

        // 向量索引重建由调用方通过 RebuildVectorsTask 触发
        Ok(current_provider)
    }
}
