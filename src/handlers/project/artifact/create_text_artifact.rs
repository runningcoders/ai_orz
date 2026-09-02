//! Handler: create_text_artifact - Create a text-based artifact with content

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, CreateTextArtifactParams};
use common::enums::FileType;
use common::error::{Result, bail_err};

/// Create a text-based artifact with content.
///
/// Agent provides text content directly; the tool handles file creation
/// and artifact metadata registration in one step.
#[register_handler_tool(
    id = "create_text_artifact",
    name = "Create Text Artifact",
    description = "Create a text artifact from inline content in one step: the text is saved to artifact storage and registered under the project, defaulting file_name to '<name>.md' and MIME type to text/plain (max 1 MB). Returns the artifact detail. For attachment-based artifacts use create_artifact; for files already on disk use register_artifact_from_path.",
    params = "common::api::CreateTextArtifactParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_text_artifact(
    ctx: RequestContext,
    params: CreateTextArtifactParams,
) -> Result<ArtifactDetail> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.project_id.trim().is_empty() {
        bail_err!(InvalidRequest, "project_id不能为空");
    }
    if params.name.trim().is_empty() {
        bail_err!(InvalidRequest, "name不能为空");
    }

    // Validate content size (max 1MB for text)
    let content_bytes = params.content.into_bytes();
    if content_bytes.len() > 1024 * 1024 {
        bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
    }

    let file_name = params
        .file_name
        .unwrap_or_else(|| format!("{}.md", params.name));
    let mime_type = params.mime_type.unwrap_or_else(|| "text/plain".to_string());
    let file_type = params.file_type.unwrap_or(FileType::Document);

    let artifact = project::domain()
        .artifact_manage()
        .create_generated_artifact(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            content_bytes,
            file_name,
            mime_type,
            file_type,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&artifact))
}
