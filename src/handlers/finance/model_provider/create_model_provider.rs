//! Handler: POST /api/v1/model-providers - Create a new model provider

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{CreateModelProviderRequest, CreateModelProviderResponse};
use crate::error::AppError;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Create a new model provider configuration for AI inference
#[register_handler_tool(
    id = "create_model_provider",
    name = "create_model_provider",
    description = "Create a new model provider configuration for AI inference",
    params = "common::api::CreateModelProviderRequest",
)]
#[generate_http_handler]
pub async fn create_model_provider(
    ctx: RequestContext,
    params: CreateModelProviderRequest,
) -> Result<CreateModelProviderResponse, AppError> {
    let provider_po = ModelProviderPo::new(
        params.name.clone(),
        params.provider_type,
        params.capability,
        params.model_name.clone(),
        params.api_key.clone(),
        params.base_url.clone(),
        params.description.clone(),
        ctx.uid().to_string(),
    );
    let provider = ModelProvider::from_po(provider_po);

    domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &provider)
        .await?;

    Ok(CreateModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        model_name: provider.po.model_name.clone(),
        description: if provider
            .po
            .description
            .as_ref()
            .map_or(true, |d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        created_at: provider.po.created_at,
    })
}