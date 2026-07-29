//! Handler: POST /api/v1/project/artifacts - Create a new artifact

use super::response;
use crate::models::attachment::AttachmentGetOptions;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::domain::{finance, project};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateArtifactRequest, CreateArtifactResponse};
use common::enums::ArtifactSourceType;
use common::error::{Result, bail_err, err};

/// Create a new artifact (from existing attachment or generated content)
#[register_handler_tool(
    id = "create_artifact",
    name = "create_artifact",
    description = "Create a new artifact in a project, supports creating from existing attachment or generated content",
    params = "common::api::CreateArtifactRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_artifact(
    ctx: RequestContext,
    params: CreateArtifactRequest,
) -> Result<CreateArtifactResponse> {
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

    let artifact = match params.source_type {
        ArtifactSourceType::Attachment => {
            create_from_attachment(ctx, params, current_user_id).await?
        }
        ArtifactSourceType::GeneratedContent => {
            create_from_generated_content(ctx, params, current_user_id).await?
        }
        ArtifactSourceType::RemoteUrl => {
            bail_err!(
                UnsupportedOperation,
                "remote_url artifact create is reserved for future extension"
            );
        }
    };

    Ok(response::to_detail(&artifact))
}

async fn create_from_attachment(
    ctx: RequestContext,
    params: CreateArtifactRequest,
    current_user_id: String,
) -> Result<crate::models::artifact::Artifact> {
    let attachment_id = params
        .attachment_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| err!(InvalidRequest, "attachment_id 不能为空"))?;

    if params.content.is_some() || params.file_name.is_some() || params.mime_type.is_some() {
        bail_err!(
            InvalidRequest,
            "attachment 类型产物不能同时携带 content/file_name/mime_type"
        );
    }

    let attachment = finance::domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), attachment_id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", attachment_id))?;

    let file_type = params.file_type.unwrap_or(attachment.po.file_type);
    let file_meta = FileMeta::new(
        format!("attachments/{}", attachment.po.relative_path),
        attachment.po.mime_type.clone(),
        attachment.po.size as u64,
    );

    project::domain()
        .artifact_manage()
        .create_attachment_artifact(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            file_type,
            file_meta,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await
}

async fn create_from_generated_content(
    ctx: RequestContext,
    params: CreateArtifactRequest,
    current_user_id: String,
) -> Result<crate::models::artifact::Artifact> {
    let content = params.content
        .ok_or_else(|| err!(InvalidRequest, "content 不能为空（generated_content 类型）"))?;
    let file_name = params.file_name
        .ok_or_else(|| err!(InvalidRequest, "file_name 不能为空（generated_content 类型）"))?;

    // Validate content size (max 1MB for text)
    let content_bytes = content.into_bytes();
    if content_bytes.len() > 1024 * 1024 {
        bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
    }

    let mime_type = params.mime_type.unwrap_or_else(|| "text/plain".to_string());
    let file_type = params.file_type.unwrap_or(common::enums::FileType::Document);

    project::domain()
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
        .await
}
