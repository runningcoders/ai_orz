//! 删除 Attachment

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, EmptyResponse};

use common::error::{err, bail_err, Result};
use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 删除 Attachment
/// DELETE /attachments/{id}
pub async fn delete_attachment(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<EmptyResponse>>> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), &id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", id))?;

    if attachment.po.root_user_id != user_id {
        bail_err!(NotFound, "Attachment {} not found", id);
    }

    domain()
        .attachment_manage()
        .delete_attachment(ctx, &id)
        .await?;

    Ok(Json(ApiResponse::success(EmptyResponse {})))
}
