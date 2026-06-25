//! Handler: GET /api/v1/project/artifacts/{id} - Get artifact basic information

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetArtifactRequest, GetArtifactResponse};
use common::bail_err;

/// Get artifact detailed information by ID
#[register_handler_tool(
    id = "get_artifact",
    name = "get_artifact",
    description = "Get detailed metadata information about an artifact",
    params = "common::api::GetArtifactRequest"
)]
#[generate_http_handler]
pub async fn get_artifact(
    ctx: RequestContext,
    params: GetArtifactRequest,
) -> Result<GetArtifactResponse> {
    let artifact = project::domain()
        .artifact_manage()
        .get(ctx, &params.artifact_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Artifact {} not found", params.artifact_id)))?;

    Ok(response::to_detail(&artifact))
}
