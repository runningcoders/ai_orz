//! Handler: GET /api/v1/attachments - 列出当前用户的 Attachment

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AttachmentDetail, AttachmentListQuery, PagedResult};
use common::error::bail_err;

use super::response::to_detail;

/// 列出当前用户（root_user_id）的 Attachment
#[register_handler_tool(
    id = "list_attachments",
    name = "list_attachments",
    description = "List attachments for the current user with optional filtering by purpose and file_type. Returns paged results.",
    params = "common::api::AttachmentListQuery",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn list_attachments(
    ctx: RequestContext,
    params: AttachmentListQuery,
) -> Result<PagedResult<AttachmentDetail>> {
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
                purpose: params.purpose.clone(),
                file_type: params.file_type,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|a| to_detail(&a)))
}
