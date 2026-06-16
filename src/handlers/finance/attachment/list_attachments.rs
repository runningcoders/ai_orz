//! 列出 Attachment

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, AttachmentDetail, AttachmentListQuery};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 列出 Attachment
/// GET /attachments
pub async fn list_attachments(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<AttachmentListQuery>,
) -> Result<Json<ApiResponse<Vec<AttachmentDetail>>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let attachments = domain()
        .attachment_manage()
        .query_attachments(
            ctx,
            AttachmentQuery {
                root_user_id: Some(user_id),
                purpose: req.purpose.clone(),
                file_type: req.file_type,
                limit: req.limit,
            },
        )
        .await?;

    let responses = attachments.iter().map(to_detail).collect();
    Ok(Json(ApiResponse::success(responses)))
}
