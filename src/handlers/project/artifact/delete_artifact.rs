//! 删除 Artifact

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::ApiResponse;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project;

/// 删除 Artifact
/// DELETE /api/v1/project/artifacts/{id}
pub async fn delete_artifact(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    project::domain()
        .artifact_manage()
        .get(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {} not found", id)))?;

    project::domain().artifact_manage().delete(ctx, &id).await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
