//! 列出 Attachment

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, AttachmentDetail, AttachmentListQuery};

use common::bail_err;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::Result;
use common::err;

/// 列出 Attachment
/// GET /attachments
pub async fn list_attachments(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<AttachmentListQuery>,
) -> Result<Json<ApiResponse<Vec<AttachmentDetail>>>> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
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