//! Handler: DELETE /api/v1/model-providers/{id} - Delete a model provider

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{DeleteModelProviderRequest, DeleteModelProviderResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Delete an existing model provider configuration
#[register_handler_tool(
    id = "delete_model_provider",
    name = "delete_model_provider",
    description = "Delete an existing model provider configuration",
    params = "common::api::DeleteModelProviderRequest",
)]
#[generate_http_handler]
pub async fn delete_model_provider(
    ctx: RequestContext,
    params: DeleteModelProviderRequest,
) -> Result<DeleteModelProviderResponse, AppError> {
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", params.id)))?;

    domain()
        .model_provider_manage()
        .delete_model_provider(ctx, &provider)
        .await?;

    Ok(DeleteModelProviderResponse { success: true })
}