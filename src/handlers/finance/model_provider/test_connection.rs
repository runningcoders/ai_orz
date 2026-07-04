//! Handler: POST /api/v1/model-providers/{id}/test - Test model provider connectivity

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{TestConnectionResponse, TestModelProviderConnectionRequest};

use crate::enrich_ctx;

/// Test connectivity and authentication to a model provider with a sample prompt
#[register_handler_tool(
    id = "test_model_provider_connection",
    name = "test_model_provider_connection",
    description = "Test connectivity and authentication to a model provider with a sample prompt",
    params = "common::api::TestModelProviderConnectionRequest"
)]
#[generate_http_handler]
pub async fn test_model_provider_connection(
    ctx: RequestContext,
    params: TestModelProviderConnectionRequest,
) -> Result<TestConnectionResponse> {
    // 1. Get the model provider
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &provider);

    // 2. Use prompt for connection test, default to "Hello!"
    let prompt = params
        .prompt
        .clone()
        .unwrap_or_else(|| "Hello!".to_string());

    match domain()
        .model_provider_manage()
        .test_connection(ctx, &provider, &prompt)
        .await
    {
        Ok(result) => {
            // Empty result also counts as failed
            if result.trim().is_empty() {
                Ok(TestConnectionResponse {
                    success: false,
                    response: Some("模型返回空响应，连通性测试不通过".to_string()),
                    error: None,
                })
            } else {
                Ok(TestConnectionResponse {
                    success: true,
                    response: Some(result),
                    error: None,
                })
            }
        }
        Err(e) => Ok(TestConnectionResponse {
            success: false,
            response: None,
            error: Some(e.to_string()),
        }),
    }
}
