//! Handler: PUT /api/v1/project/artifacts/{id}/content - Update artifact text content

use crate::error::AppError;
use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, UpdateArtifactContentRequest};

/// Update the full text content of a generated-content artifact (full replace)
#[register_handler_tool(
    id = "update_artifact_content",
    name = "update_artifact_content",
    description = "Fully replace the text content of a generated-content artifact, supports optimistic locking with expected_updated_at",
    params = "common::api::UpdateArtifactContentRequest"
)]
#[generate_http_handler]
pub async fn update_artifact_content(
    ctx: RequestContext,
    params: UpdateArtifactContentRequest,
) -> Result<ArtifactDetail, AppError> {
    let domain = crate::service::domain::project::domain();
    // Convert String content to bytes
    let content_bytes = params.content.into_bytes();
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
            &params.artifact_id,
            content_bytes,
            params.expected_updated_at,
        )
        .await?;

    let detail = response::to_detail(&updated_artifact);
    Ok(detail)
}
