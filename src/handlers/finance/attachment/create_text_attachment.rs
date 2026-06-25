//! 创建小型 UTF-8 文本 Attachment

use axum::{Json, extract::Extension, http::StatusCode};
use common::api::{ApiResponse, CreateTextAttachmentRequest, CreateTextAttachmentResponse};

use common::bail_err;
use crate::models::attachment::TextAttachmentCreate;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::err;

/// 创建小型 UTF-8 文本 Attachment
/// POST /attachments/text
pub async fn create_text_attachment(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateTextAttachmentRequest>,
) -> std::result::Result<(StatusCode, Json<ApiResponse<CreateTextAttachmentResponse, common::error::Error>>)> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .create_text_attachment(
            ctx,
            TextAttachmentCreate {
                file_name: req.file_name,
                content: req.content,
                mime_type: req.mime_type,
                purpose: req.purpose,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(to_detail(&attachment))),
    ))
}