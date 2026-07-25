//! 列出 Attachment

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, AttachmentDetail, AttachmentListQuery, PagedResult};

use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, bail_err};

/// 列出 Attachment
/// GET /attachments
pub async fn list_attachments(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<AttachmentListQuery>,
) -> Result<Json<ApiResponse<PagedResult<AttachmentDetail>>>> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let page = domain()
        .attachment_manage()
        .query_attachments(
            ctx,
            AttachmentQuery {
                root_user_id: Some(user_id),
                purpose: req.purpose.clone(),
                file_type: req.file_type,
                pagination: req.pagination,
            },
        )
        .await?;

    let paged = page.map(|a| to_detail(&a));
    Ok(Json(ApiResponse::success(paged)))
}