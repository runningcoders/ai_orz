//! Handler: GET /api/v1/project/artifacts/{id}/content - Get artifact text content

use crate::error::AppError;
use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::GetArtifactContentRequest;
use common::api::artifact::{ArtifactContentText, GetArtifactContentResponse};

/// Get the full text content of a generated-content artifact
#[register_handler_tool(
    id = "get_artifact_content",
    name = "get_artifact_content",
    description = "Get the text content of an artifact with source_type = generated_content",
    params = "common::api::GetArtifactContentRequest"
)]
#[generate_http_handler]
pub async fn get_artifact_content(
    ctx: RequestContext,
    params: GetArtifactContentRequest,
) -> Result<GetArtifactContentResponse, AppError> {
    let domain = project::domain();
    let result = domain
        .artifact_manage()
        .get_artifact_content(ctx.clone(), &params.artifact_id)
        .await?;

    match result {
        None => Err(AppError::NotFound(format!(
            "Artifact not found or no content available: {}",
            params.artifact_id
        ))),
        Some((artifact, content_bytes)) => {
            // Validate that content is valid UTF-8
            let content_str = String::from_utf8(content_bytes).map_err(|_| {
                AppError::BadRequest(format!(
                    "Artifact content is not valid UTF-8 text: {}",
                    params.artifact_id
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

            Ok(response)
        }
    }
}
