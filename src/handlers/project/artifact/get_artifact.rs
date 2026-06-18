//! Handler: GET /api/v1/project/artifacts/{id} - Get artifact basic information

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{GetArtifactRequest, GetArtifactResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use super::response;

/// Get artifact detailed information by ID
#[register_handler_tool(
    id = "get_artifact",
    name = "get_artifact",
    description = "Get detailed metadata information about an artifact",
    params = "common::api::GetArtifactRequest",
)]
#[generate_http_handler]
pub async fn get_artifact(
    ctx: RequestContext,
    params: GetArtifactRequest,
) -> Result<GetArtifactResponse, AppError> {
    let artifact = project::domain()
        .artifact_manage()
        .get(ctx, &params.artifact_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {} not found", params.artifact_id)))?;

    Ok(response::to_detail(&artifact))
}