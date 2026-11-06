//! Handler: POST /api/v1/finance/model-providers/:id/switch - Switch embedding provider

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SwitchEmbeddingProviderRequest, SwitchEmbeddingProviderResponse};
use common::error::{Error, Result};

/// Switch embedding provider (requires user confirmation)
#[generate_http_handler]
pub async fn switch_embedding_provider(
    ctx: RequestContext,
    params: SwitchEmbeddingProviderRequest,
) -> Result<SwitchEmbeddingProviderResponse> {
    if !params.confirm {
        return Err(Error::bad_request(
            "Confirmation required - set confirm: true to proceed",
        ));
    }

    let previous_provider = domain()
        .model_provider_manage()
        .switch_embedding_provider(ctx.clone(), &params.id)
        .await?;

    let new_provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    // 切换只负责「禁用旧 + 启用新」，是否重建向量索引与切换解耦：
    // 两个模型可能只是计费方式/接入参数不同（向量空间一致），自动重建会产生无意义开销。
    // 需要重建时由前端二次确认后显式调用 POST /api/v1/system/vector-rebuild。

    Ok(SwitchEmbeddingProviderResponse {
        id: new_provider.po.id.clone(),
        name: new_provider.po.name.clone(),
        previous_provider_id: previous_provider.as_ref().map(|p| p.po.id.clone()),
        previous_provider_name: previous_provider.as_ref().map(|p| p.po.name.clone()),
    })
}
