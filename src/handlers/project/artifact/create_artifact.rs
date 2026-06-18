//! Handler: POST /api/v1/project/artifacts - Create a new artifact

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{CreateArtifactRequest, CreateArtifactResponse};
use common::enums::ArtifactSourceType;
use crate::error::AppError;
use crate::models::attachment::AttachmentGetOptions;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::domain::{finance, project};
use super::response;

/// Create a new artifact (from existing attachment or generated content)
#[register_handler_tool(
    id = "create_artifact",
    name = "create_artifact",
    description = "Create a new artifact in a project, supports creating from existing attachment or reserved for generated content",
    params = "common::api::CreateArtifactRequest",
)]
#[generate_http_handler]
pub async fn create_artifact(
    ctx: RequestContext,
    params: CreateArtifactRequest,
) -> Result<CreateArtifactResponse, AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }
    if params.project_id.trim().is_empty() {
        return Err(AppError::BadRequest("project_id 不能为空".to_string()));
    }
    if params.name.trim().is_empty() {
        return Err(AppError::BadRequest("产物名称不能为空".to_string()));
    }

    let artifact = match params.source_type {
        ArtifactSourceType::Attachment => {
            create_from_attachment(ctx, params, current_user_id).await?
        }
        ArtifactSourceType::GeneratedContent => {
            return Err(AppError::Unsupported(
                "generated_content artifact create is not implemented yet".to_string(),
            ));
        }
        ArtifactSourceType::RemoteUrl => {
            return Err(AppError::Unsupported(
                "remote_url artifact create is reserved for future extension".to_string(),
            ));
        }
    };

    Ok(response::to_detail(&artifact))
}

async fn create_from_attachment(
    ctx: RequestContext,
    params: CreateArtifactRequest,
    current_user_id: String,
) -> Result<crate::models::artifact::Artifact, AppError> {
    let attachment_id = params
        .attachment_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("attachment_id 不能为空".to_string()))?;

    if params.content.is_some() || params.file_name.is_some() || params.mime_type.is_some() {
        return Err(AppError::BadRequest(
            "attachment 类型产物不能同时携带 content/file_name/mime_type".to_string(),
        ));
    }

    let attachment = finance::domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), attachment_id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", attachment_id)))?;

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