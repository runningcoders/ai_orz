//! Handler: POST /api/v1/model-providers/{id}/call - Call model to generate text completion

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CallModelRequest, CallModelResponse};
use common::error::Result;

use crate::enrich_ctx;

/// Call a configured model provider to generate text completion with a given prompt
#[register_handler_tool(
    id = "call_model",
    name = "Call LLM Model",
    description = "Send a prompt to a configured model provider (id selects the provider, its model, and credentials) and return the generated completion synchronously in {result}. Fails if the provider does not exist or the upstream call errors; use test_model_provider_connection for a lightweight reachability check.",
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
        .ok_or_else(|| {
            common::error::Error::not_found(format!("ModelProvider {} not found", params.id))
        })?;

    let ctx = enrich_ctx!(&ctx, &provider);

    // 2. Call the model to generate result
    let result = domain()
        .model_provider_manage()
        .test_connection(ctx, &provider, &params.prompt)
        .await?;

    Ok(CallModelResponse { result })
}
