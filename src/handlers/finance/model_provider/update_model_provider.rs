//! Handler: PUT /api/v1/model-providers/{id} - Update model provider configuration

use crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask;
use crate::pkg::RequestContext;
use crate::pkg::background_task::registry;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateModelProviderRequest, UpdateModelProviderResponse};
use common::enums::ModelProviderStatus;
use common::error::Result;
use std::sync::Arc;

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

    // Embedding 配置变化检测：仅「使用中(Normal)」的配置变化需要重建向量索引；
    // Disabled 的编辑不重建（启用切换时 switch 全量重建兜底）。
    // 注意：status 判断必须用更新前的值（在 params.status 应用前记录）。
    let was_enabled_embedding =
        provider.po.capability.is_embedding() && provider.po.status == ModelProviderStatus::Normal;
    let embedding_config_changed = was_enabled_embedding
        && (params
            .model_name
            .as_deref()
            .is_some_and(|v| v != provider.po.model_name)
            || params
                .api_key
                .as_deref()
                .is_some_and(|v| v != provider.po.api_key)
            || params
                .base_url
                .as_deref()
                .is_some_and(|v| provider.po.base_url.as_deref() != Some(v)));

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
    // 上下文长度配置 partial update：None 不修改，Some(0) 清除，Some(n>0) 设置
    if params.max_context_length.is_some() || params.recommended_context_length.is_some() {
        provider.po.update_config(|cfg| {
            if let Some(v) = params.max_context_length {
                cfg.max_context_length = if v > 0 { Some(v) } else { None };
            }
            if let Some(v) = params.recommended_context_length {
                cfg.recommended_context_length = if v > 0 { Some(v) } else { None };
            }
        });
    }
    // Update modified_by and updated_at
    provider.po.modified_by = ctx.uid();
    provider.po.updated_at = current_timestamp();

    domain()
        .model_provider_manage()
        .update_model_provider(ctx.clone(), &provider)
        .await?;

    // 使用中 Embedding 的配置变化 → 向量空间变化，注册全量重建。
    // 本路径触发的重建不软删旧 provider（同模型原地改配置），与 switch 语义不同，属预期。
    let rebuild_task_id = if embedding_config_changed {
        let task = Arc::new(RebuildVectorsTask::new(ctx));
        Some(registry().register(task).await)
    } else {
        None
    };

    let config = provider.po.config();
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
        rebuild_task_id,
        max_context_length: config.max_context_length,
        recommended_context_length: config.recommended_context_length,
    })
}
