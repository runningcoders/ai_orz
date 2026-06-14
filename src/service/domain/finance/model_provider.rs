//! Model Provider 具体实现

use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::domain::finance::{FinanceDomainImpl, ModelProviderManage};

#[async_trait::async_trait]
impl ModelProviderManage for FinanceDomainImpl {
    async fn create_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), crate::error::AppError> {
        self.model_provider_dal.create(ctx, provider).await
    }

    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>, crate::error::AppError> {
        self.model_provider_dal.find_by_id(ctx, id).await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>, crate::error::AppError> {
        self.model_provider_dal.query(ctx, query).await
    }

    async fn list_model_providers(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<ModelProvider>, crate::error::AppError> {
        self.model_provider_dal.find_all(ctx).await
    }

    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), crate::error::AppError> {
        self.model_provider_dal.update(ctx, provider).await
    }

    async fn delete_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), crate::error::AppError> {
        self.model_provider_dal.delete(ctx, provider).await
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String, crate::error::AppError> {
        self.brain_dal.test_connection(ctx, provider, prompt).await
    }
}
