//! 获取单个 Attachment

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetAttachmentResponse};

use crate::error::AppError;
use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 获取 Attachment
/// GET /attachments/{id}
pub async fn get_attachment(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetAttachmentResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx, &id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", id)))?;

    if attachment.po.root_user_id != user_id {
        return Err(AppError::NotFound(format!("Attachment {} not found", id)));
    }

    Ok(Json(ApiResponse::success(to_detail(&attachment))))
}
