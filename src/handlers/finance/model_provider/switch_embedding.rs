//! Handler: POST /api/v1/finance/model-providers/:id/switch - Switch embedding provider

use crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask;
use crate::pkg::RequestContext;
use crate::pkg::background_task::registry;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SwitchEmbeddingProviderRequest, SwitchEmbeddingProviderResponse};
use common::error::{Error, Result};
use std::sync::Arc;

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

    // 同一 provider（domain 层已提前返回）→ 无需重建；
    // 否则注册向量重建任务到全局 registry
    let (rebuild_status, task_id) = match &previous_provider {
        Some(p) if p.po.id == params.id => ("completed".to_string(), String::new()),
        _ => {
            let task = Arc::new(RebuildVectorsTask::new(ctx));
            let task_id = registry().register(task).await;
            ("running".to_string(), task_id)
        }
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
