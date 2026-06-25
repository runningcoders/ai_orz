//! Handler: DELETE /api/v1/model-providers/{id} - Delete a model provider

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteModelProviderRequest, DeleteModelProviderResponse};
use common::bail_err;

/// Delete an existing model provider configuration
#[register_handler_tool(
    id = "delete_model_provider",
    name = "delete_model_provider",
    description = "Delete an existing model provider configuration",
    params = "common::api::DeleteModelProviderRequest"
)]
#[generate_http_handler]
pub async fn delete_model_provider(
    ctx: RequestContext,
    params: DeleteModelProviderRequest,
) -> Result<DeleteModelProviderResponse> {
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    domain()
        .model_provider_manage()
        .delete_model_provider(ctx, &provider)
        .await?;

    Ok(DeleteModelProviderResponse { success: true })
}
