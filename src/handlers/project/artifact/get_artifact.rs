//! 获取 Artifact 详情

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetArtifactResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project;

use super::response;

/// 获取 Artifact 详情
/// GET /api/v1/project/artifacts/{id}
pub async fn get_artifact(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetArtifactResponse>>, AppError> {
    let artifact = project::domain()
        .artifact_manage()
        .get(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {} not found", id)))?;

    Ok(Json(ApiResponse::success(response::to_detail(&artifact))))
}
