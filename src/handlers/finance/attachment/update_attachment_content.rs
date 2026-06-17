//! 全量替换 Attachment UTF-8 文本内容

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, AttachmentContentResponse, UpdateTextContentRequest};

use crate::error::AppError;
use crate::models::attachment::TextContentUpdate;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_content_response;

/// 全量替换 Attachment UTF-8 文本内容
/// PUT /attachments/{id}/content
pub async fn update_attachment_content(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTextContentRequest>,
) -> Result<Json<ApiResponse<AttachmentContentResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let content = domain()
        .attachment_manage()
        .update_attachment_text_content(
            ctx,
            &id,
            TextContentUpdate {
                content: req.content,
                expected_updated_at: req.expected_updated_at,
            },
        )
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", id)))?;

    Ok(Json(ApiResponse::success(to_content_response(&content))))
}
