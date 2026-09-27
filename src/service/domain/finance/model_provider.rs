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
        // Embedding「创建不阻塞 + 启用时切换」：已有启用的 Embedding Provider 时，
        // 新的允许创建但降级为 Disabled（未启用）；启用时走 update 守卫 409
        // → 前端 switch modal 确认 → switch_embedding（软删旧+启用新+全量重建）。
        // 首个 Embedding 直接 Normal 启用（当前无任何启用者）。
        let mut provider = provider.clone();
        if provider.po.capability.is_embedding()
            && provider.po.status == ModelProviderStatus::Normal
            && self
                .model_provider_dal
                .find_enabled_embedding_provider(ctx.clone())
                .await?
                .is_some()
        {
            provider.po.status = ModelProviderStatus::Disabled;
        }

        // Decision（小脑）单启用：与 Embedding 同构的「创建不阻塞」，已有启用者时
        // 新记录静默降级 Disabled；首个 Decision 直接 Normal 启用。
        // 切换形态见 update 守卫：静默自动停用旧记录（无 409 / 无 switch modal /
        // 无重建——小脑切换零重建成本，选路逻辑下次自然选到新模型）。
        if provider.po.capability.is_decision()
            && provider.po.status == ModelProviderStatus::Normal
            && self
                .model_provider_dal
                .find_enabled_decision_provider(ctx.clone())
                .await?
                .is_some()
        {
            provider.po.status = ModelProviderStatus::Disabled;
        }

        let ctx = enrich_ctx!(&ctx, &provider);
        self.model_provider_dal.create(ctx, &provider).await
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

        // Decision（小脑）「随时切换零额外操作」（AMan 澄清口径）：启用某条
        // Decision 记录时，静默自动停用旧的启用记录——不做 409 / switch modal /
        // 全量重建（小脑切换零重建成本，选路逻辑下次选到新模型即可）。
        if provider.po.capability.is_decision()
            && provider.po.status == ModelProviderStatus::Normal
            && let Some(mut current) = self
                .model_provider_dal
                .find_enabled_decision_provider(ctx.clone())
                .await?
            && current.po.id != provider.po.id
        {
            current.po.status = ModelProviderStatus::Disabled;
            self.model_provider_dal
                .update(ctx.clone(), &current)
                .await?;
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

    async fn model_call_time_series(
        &self,
        ctx: RequestContext,
        minutes: u32,
    ) -> Result<Vec<common::models::TimeSeriesPoint>> {
        self.model_provider_dal
            .model_call_time_series(ctx, minutes)
            .await
    }

    async fn get_model_call_stats_for_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
        options: common::models::StatsFetchOptions,
    ) -> Result<common::models::ModelCallStats> {
        self.model_provider_dal
            .get_model_call_stats_for_user(ctx, user_id, options)
            .await
    }
}
