//! Update artifact content handler
//!
//! PUT /api/v1/project/artifacts/{id}/content
//! Fully replaces the text content of a generated-content artifact.

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::ApiResponse;
use common::api::artifact::{ArtifactDetail, UpdateArtifactContentRequest};

use crate::error::AppError;
use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;

/// PUT /api/v1/project/artifacts/{artifact_id}/content
pub async fn update_artifact_content(
    Extension(ctx): Extension<RequestContext>,
    Path(artifact_id): Path<String>,
    Json(req): Json<UpdateArtifactContentRequest>,
) -> Result<Json<ApiResponse<ArtifactDetail>>, AppError> {
    let domain = crate::service::domain::project::domain();
    // Convert String content to bytes
    let content_bytes = req.content.into_bytes();
    // Validate content size (max 1MB for text)
    if content_bytes.len() > 1024 * 1024 {
        return Err(AppError::BadRequest(
            "Text content exceeds maximum size of 1MB".to_string(),
        ));
    }

    let updated_artifact = domain
        .artifact_manage()
        .update_artifact_content(
            ctx.clone(),
            &artifact_id,
            content_bytes,
            req.expected_updated_at,
        )
        .await?;

    let detail = response::to_detail(&updated_artifact);
    Ok(Json(ApiResponse::success(detail)))
}
