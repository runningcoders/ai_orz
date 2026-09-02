//! Handler: DELETE /api/v1/project/artifacts/{id} - Delete an artifact

use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteArtifactRequest, DeleteArtifactResponse};
use common::error::Result;

/// Delete an existing artifact by ID
#[register_handler_tool(
    id = "delete_artifact",
    name = "Delete Artifact",
    description = "Soft-delete a project artifact by ID: the record is marked deleted (excluded from later listings) while stored content is kept. Returns success: true. Fails with not found if the artifact does not exist.",
    params = "common::api::DeleteArtifactRequest"
)]
#[generate_http_handler]
pub async fn delete_artifact(
    ctx: RequestContext,
    params: DeleteArtifactRequest,
) -> Result<DeleteArtifactResponse> {
    project::domain()
        .artifact_manage()
        .get(ctx.clone(), &params.artifact_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Artifact {} not found", params.artifact_id))
        })?;

    project::domain()
        .artifact_manage()
        .delete(ctx, &params.artifact_id)
        .await?;

    Ok(DeleteArtifactResponse { success: true })
}
