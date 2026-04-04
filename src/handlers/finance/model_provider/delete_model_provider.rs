//! 删除 Model Provider

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::finance::domain;
use axum::{
    extract::Path,
    http::HeaderMap,
    Json,
};

/// 删除 Model Provider
/// DELETE /model-providers/{id}
pub async fn delete_model_provider(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let ctx = extract_ctx(&headers);

    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    domain().model_provider_manage().delete_model_provider(ctx, &provider)?;

    Ok(Json(ApiResponse::<()>::ok()))
}
