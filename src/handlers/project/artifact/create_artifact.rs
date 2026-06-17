//! 创建 Artifact

use axum::{Json, extract::Extension, http::StatusCode};
use common::api::{ApiResponse, CreateArtifactRequest, CreateArtifactResponse};
use common::enums::ArtifactSourceType;

use crate::error::AppError;
use crate::models::attachment::AttachmentGetOptions;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::domain::{finance, project};

use super::response;

/// 创建 Artifact
/// POST /api/v1/project/artifacts
pub async fn create_artifact(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateArtifactRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateArtifactResponse>>), AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }
    if req.project_id.trim().is_empty() {
        return Err(AppError::BadRequest("project_id 不能为空".to_string()));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("产物名称不能为空".to_string()));
    }

    let artifact = match req.source_type {
        ArtifactSourceType::Attachment => create_from_attachment(ctx, req, current_user_id).await?,
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

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(response::to_detail(&artifact))),
    ))
}

async fn create_from_attachment(
    ctx: RequestContext,
    req: CreateArtifactRequest,
    current_user_id: String,
) -> Result<crate::models::artifact::Artifact, AppError> {
    let attachment_id = req
        .attachment_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("attachment_id 不能为空".to_string()))?;

    if req.content.is_some() || req.file_name.is_some() || req.mime_type.is_some() {
        return Err(AppError::BadRequest(
            "attachment 类型产物不能同时携带 content/file_name/mime_type".to_string(),
        ));
    }

    let attachment = finance::domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), attachment_id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", attachment_id)))?;

    let file_type = req.file_type.unwrap_or(attachment.po.file_type);
    let file_meta = FileMeta::new(
        format!("attachments/{}", attachment.po.relative_path),
        attachment.po.mime_type.clone(),
        attachment.po.size as u64,
    );

    project::domain()
        .artifact_manage()
        .create_attachment_artifact(
            ctx,
            req.project_id,
            req.task_id,
            req.name,
            req.description.unwrap_or_default(),
            file_type,
            file_meta,
            req.tags.unwrap_or_default(),
            current_user_id,
        )
        .await
}
