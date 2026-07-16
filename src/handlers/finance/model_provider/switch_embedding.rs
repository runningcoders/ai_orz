//! Handler: POST /api/v1/finance/model-providers/:id/switch - Switch embedding provider

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SwitchEmbeddingProviderRequest, SwitchEmbeddingProviderResponse};

/// Switch embedding provider (requires user confirmation)
#[generate_http_handler]
pub async fn switch_embedding_provider(
    ctx: RequestContext,
    params: SwitchEmbeddingProviderRequest,
) -> Result<SwitchEmbeddingProviderResponse> {
    if !params.confirm {
        return Err(Error::bad_request("Confirmation required - set confirm: true to proceed"));
    }

    let (previous_provider, task_id) = domain()
        .model_provider_manage()
        .switch_embedding_provider(ctx.clone(), &params.id)
        .await?;

    let new_provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx, &params.id)
        .await?
        .ok_or_else(|| Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    let rebuild_status = if task_id.is_empty() {
        "completed".to_string()
    } else {
        "running".to_string()
    };

    Ok(SwitchEmbeddingProviderResponse {
        id: new_provider.po.id.clone(),
        name: new_provider.po.name.clone(),
        previous_provider_id: previous_provider.as_ref().map(|p| p.po.id.clone()),
        previous_provider_name: previous_provider.as_ref().map(|p| p.po.name.clone()),
        rebuild_status,
        task_id,
    })
}
