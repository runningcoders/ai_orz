//! Handler: POST /api/v1/model-providers - Create a new model provider

use crate::models::model_provider::{ModelProvider, ModelProviderConfig, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateModelProviderRequest, CreateModelProviderResponse};
use common::error::Result;

/// Create a new model provider configuration for AI inference
#[register_handler_tool(
    id = "create_model_provider",
    name = "create_model_provider",
    description = "Create a new model provider configuration for AI inference",
    params = "common::api::CreateModelProviderRequest"
)]
#[generate_http_handler]
pub async fn create_model_provider(
    ctx: RequestContext,
    params: CreateModelProviderRequest,
) -> Result<CreateModelProviderResponse> {
    let mut provider_po = ModelProviderPo::new(
        params.name.clone(),
        params.provider_type,
        params.capability,
        params.model_name.clone(),
        params.api_key.clone(),
        params.base_url.clone(),
        params.description.clone(),
        ctx.uid().to_string(),
    );

    // 写入上下文长度配置（0 视为未设置）
    let config = ModelProviderConfig {
        max_context_length: params.max_context_length.filter(|&v| v > 0),
        recommended_context_length: params.recommended_context_length.filter(|&v| v > 0),
        ..Default::default()
    };
    provider_po.set_config(&config);

    let provider = ModelProvider::from_po(provider_po);

    domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &provider)
        .await?;

    // 回读落库状态（domain 对已有启用者的 embedding 创建降级为 Disabled）
    let created = domain()
        .model_provider_manage()
        .get_model_provider(ctx, &provider.po.id)
        .await?
        .ok_or_else(|| common::error::Error::internal("created provider not found after create"))?;

    let config = created.po.config();
    Ok(CreateModelProviderResponse {
        id: created.po.id.clone(),
        name: created.po.name.clone(),
        provider_type: created.po.provider_type,
        model_name: created.po.model_name.clone(),
        description: if created.po.description.as_ref().is_none_or(|d| d.is_empty()) {
            None
        } else {
            created.po.description.clone()
        },
        created_at: created.po.created_at,
        status: created.po.status as i32,
        max_context_length: config.max_context_length,
        recommended_context_length: config.recommended_context_length,
    })
}
