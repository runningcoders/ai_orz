//! 删除 Attachment

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, EmptyResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 删除 Attachment
/// DELETE /attachments/{id}
pub async fn delete_attachment(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<EmptyResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", id)))?;

    if attachment.po.root_user_id != user_id {
        return Err(AppError::NotFound(format!("Attachment {} not found", id)));
    }

    domain()
        .attachment_manage()
        .delete_attachment(ctx, &id)
        .await?;

    Ok(Json(ApiResponse::success(EmptyResponse {})))
}
