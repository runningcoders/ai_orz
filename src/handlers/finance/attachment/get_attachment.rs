//! 获取单个 Attachment

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetAttachmentResponse};

use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, err, bail_err};

/// 获取 Attachment
/// GET /attachments/{id}
pub async fn get_attachment(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetAttachmentResponse>>> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx, &id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", id))?;

    if attachment.po.root_user_id != user_id {
        bail_err!(NotFound, "Attachment {} not found", id);
    }

    Ok(Json(ApiResponse::success(to_detail(&attachment))))
}