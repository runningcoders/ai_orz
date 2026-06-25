//! Handler: GET /api/v1/model-providers - List all model providers

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListModelProvidersRequest, ListModelProvidersResponse, ModelProviderListItem};
use common::bail_err;

/// List all configured model providers. Returns basic information for each provider.
#[register_handler_tool(
    id = "list_model_providers",
    name = "list_model_providers",
    description = "List all configured model providers. Returns basic information for each provider.",
    params = "common::api::ListModelProvidersRequest"
)]
#[generate_http_handler]
pub async fn list_model_providers(
    ctx: RequestContext,
    _params: ListModelProvidersRequest,
) -> Result<ListModelProvidersResponse> {
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
