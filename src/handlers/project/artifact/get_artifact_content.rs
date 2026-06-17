//! Get artifact content handler
//!
//! GET /api/v1/project/artifacts/{id}/content
//! Retrieves the full text content of a generated-content artifact.

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::artifact::{ArtifactContentText, GetArtifactContentResponse};
use common::api::{ApiResponse, GetArtifactResponse};

use crate::error::AppError;
use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;

/// GET /api/v1/project/artifacts/{artifact_id}/content
pub async fn get_artifact_content(
    Extension(ctx): Extension<RequestContext>,
    Path(artifact_id): Path<String>,
) -> Result<Json<ApiResponse<GetArtifactContentResponse>>, AppError> {
    let domain = project::domain();
    let result = domain
        .artifact_manage()
        .get_artifact_content(ctx.clone(), &artifact_id)
        .await?;

    match result {
        None => Err(AppError::NotFound(format!(
            "Artifact not found or no content available: {}",
            artifact_id
        ))),
        Some((artifact, content_bytes)) => {
            // Validate that content is valid UTF-8
            let content_str = String::from_utf8(content_bytes).map_err(|_| {
                AppError::BadRequest(format!(
                    "Artifact content is not valid UTF-8 text: {}",
                    artifact_id
                ))
            })?;

            let content = ArtifactContentText {
                content: content_str,
                encoding: "utf-8".to_string(),
                size: artifact.po.file_meta.0.file_size,
                updated_at: artifact.po.updated_at,
            };

            let artifact_detail = response::to_detail(&artifact);
            let response = GetArtifactContentResponse {
                artifact: artifact_detail,
                content,
            };

            Ok(Json(ApiResponse::success(response)))
        }
    }
}
