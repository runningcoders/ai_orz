//! Handler: PUT /api/v1/model-providers/{id} - Update model provider configuration

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateModelProviderRequest, UpdateModelProviderResponse};
use common::enums::ModelProviderStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Get current timestamp
fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Update an existing model provider configuration (name, credentials, model name, etc.)
#[register_handler_tool(
    id = "update_model_provider",
    name = "update_model_provider",
    description = "Update an existing model provider configuration (name, credentials, model name, etc.)",
    params = "common::api::UpdateModelProviderRequest"
)]
#[generate_http_handler]
pub async fn update_model_provider(
    ctx: RequestContext,
    params: UpdateModelProviderRequest,
) -> Result<UpdateModelProviderResponse> {
    let mut provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("ModelProvider {} not found", params.id))
        })?;

    let ctx = enrich_ctx!(&ctx, &provider);

    // Update fields
    if let Some(name) = params.name {
        provider.po.name = name;
    }
    if let Some(provider_type) = params.provider_type {
        provider.po.provider_type = provider_type;
    }
    if let Some(model_name) = params.model_name {
        provider.po.model_name = model_name;
    }
    if let Some(api_key) = params.api_key {
        provider.po.api_key = api_key;
    }
    if let Some(base_url) = params.base_url {
        provider.po.base_url = Some(base_url);
    }
    if let Some(description) = params.description {
        provider.po.description = Some(description);
    }
    if let Some(status) = params.status {
        provider.po.status = ModelProviderStatus::from_i32(status);
    }
    // Update modified_by and updated_at
    provider.po.modified_by = ctx.uid();
    provider.po.updated_at = current_timestamp();

    domain()
        .model_provider_manage()
        .update_model_provider(ctx, &provider)
        .await?;

    Ok(UpdateModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        capability: provider.po.capability,
        model_name: provider.po.model_name.clone(),
        base_url: if provider.po.base_url.as_ref().is_none_or(|d| d.is_empty()) {
            None
        } else {
            provider.po.base_url.clone()
        },
        description: if provider
            .po
            .description
            .as_ref()
            .is_none_or(|d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        status: provider.po.status as i32,
        updated_at: provider.po.updated_at,
    })
}
