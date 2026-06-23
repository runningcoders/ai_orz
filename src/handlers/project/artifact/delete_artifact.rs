//! Handler: DELETE /api/v1/project/artifacts/{id} - Delete an artifact

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteArtifactRequest, DeleteArtifactResponse};

/// Delete an existing artifact by ID
#[register_handler_tool(
    id = "delete_artifact",
    name = "delete_artifact",
    description = "Delete an artifact from a project",
    params = "common::api::DeleteArtifactRequest"
)]
#[generate_http_handler]
pub async fn delete_artifact(
    ctx: RequestContext,
    params: DeleteArtifactRequest,
) -> Result<DeleteArtifactResponse, AppError> {
    project::domain()
        .artifact_manage()
        .get(ctx.clone(), &params.artifact_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {} not found", params.artifact_id)))?;

    project::domain()
        .artifact_manage()
        .delete(ctx, &params.artifact_id)
        .await?;

    Ok(DeleteArtifactResponse { success: true })
}
