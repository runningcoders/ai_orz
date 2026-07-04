//! Handler: POST /api/v1/model-providers/{id}/call - Call model to generate text completion

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CallModelRequest, CallModelResponse};

use crate::enrich_ctx;

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
) -> Result<CallModelResponse> {
    // 1. Get the model provider
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &provider);

    // 2. Call the model to generate result
    let result = domain()
        .model_provider_manage()
        .test_connection(ctx, &provider, &params.prompt)
        .await?;

    Ok(CallModelResponse { result })
}
