//! Handler: POST /api/v1/model-providers/{id}/call - Call model to generate text completion

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CallModelRequest, CallModelResponse};

/// Call a configured model provider to generate text completion with a given prompt
#[register_handler_tool(
    id = "call_model",
    name = "call_model",
    description = "Call a configured model provider to generate text completion with a given prompt",
    params = "common::api::CallModelRequest"
)]
#[generate_http_handler]
pub async fn call_model(
    ctx: RequestContext,
    params: CallModelRequest,
) -> Result<CallModelResponse, AppError> {
    // 1. Get the model provider
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", params.id)))?;

    // 2. Call the model to generate result
    let result = domain()
        .model_provider_manage()
        .test_connection(ctx, &provider, &params.prompt)
        .await?;

    Ok(CallModelResponse { result })
}
