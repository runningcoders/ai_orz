//! 删除 Model Provider

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::ApiResponse;

/// 删除 Model Provider
/// DELETE /model-providers/{id}
pub async fn delete_model_provider(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    domain()
        .model_provider_manage()
        .delete_model_provider(ctx, &provider)
        .await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
