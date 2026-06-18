//! Handler: GET /api/v1/model-providers - List all model providers

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{ListModelProvidersRequest, ListModelProvidersResponse, ModelProviderListItem};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// List all configured model providers. Returns basic information for each provider.
#[register_handler_tool(
    id = "list_model_providers",
    name = "list_model_providers",
    description = "List all configured model providers. Returns basic information for each provider.",
    params = "common::api::ListModelProvidersRequest",
)]
#[generate_http_handler]
pub async fn list_model_providers(
    ctx: RequestContext,
    _params: ListModelProvidersRequest,
) -> Result<ListModelProvidersResponse, AppError> {
    let providers = domain()
        .model_provider_manage()
        .list_model_providers(ctx)
        .await?;
    let providers: Vec<ModelProviderListItem> = providers
        .iter()
        .map(|provider| ModelProviderListItem {
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
        .collect();

    Ok(ListModelProvidersResponse { providers })
}