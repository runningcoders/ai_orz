//! 读取 Attachment UTF-8 文本内容

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, AttachmentContentResponse};

use common::bail_err;
use common::err;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_content_response;
use common::error::Result;

/// 读取 Attachment UTF-8 文本内容
/// GET /attachments/{id}/content
pub async fn get_attachment_content(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<AttachmentContentResponse>>> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let content = domain()
        .attachment_manage()
        .get_attachment_text_content(ctx, &id)
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", id))?;

    Ok(Json(ApiResponse::success(to_content_response(&content))))
}