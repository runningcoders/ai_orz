//! Handler: PUT /api/v1/project/artifacts/{id} - Update artifact content and/or metadata

use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, UpdateArtifactRequest};
use common::error::{Result, bail_err};

/// Update artifact content and/or metadata (partial update).
///
/// Only fields that are `Some` will be updated. Content update only applies
/// to GeneratedContent artifacts. Metadata (name/description/tags) applies to all.
#[register_handler_tool(
    id = "update_artifact",
    name = "Update Artifact",
    description = "Update artifact content and/or metadata (name, description, tags). Only provided fields are updated. Supports optimistic locking.",
    params = "common::api::UpdateArtifactRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_artifact(
    ctx: RequestContext,
    params: UpdateArtifactRequest,
) -> Result<ArtifactDetail> {
    let domain = crate::service::domain::project::domain();

    // Convert content to Option<Vec<u8>> with size validation
    let content_bytes = if let Some(content) = params.content {
        let bytes = content.into_bytes();
        if bytes.len() > 1024 * 1024 {
            bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
        }
        Some(bytes)
    } else {
        None
    };

    let updated_artifact = domain
        .artifact_manage()
        .update_artifact(
            ctx.clone(),
            &params.artifact_id,
            content_bytes,
            params.name,
            params.description,
            params.tags,
            params.expected_updated_at,
        )
        .await?;

    let detail = response::to_detail(&updated_artifact);
    Ok(detail)
}
