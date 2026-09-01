//! Handler: GET /api/v1/attachments/{id} - 获取单个 Attachment 详情

use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetAttachmentRequest, GetAttachmentResponse};
use common::error::Result;
use common::error::{bail_err, err};

use super::response::to_detail;

/// 获取 Attachment 详情
#[register_handler_tool(
    id = "get_attachment",
    name = "Get Attachment Info",
    description = "Get attachment metadata by ID. Only accessible by the owner (root_user_id).",
    params = "common::api::GetAttachmentRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn get_attachment(
    ctx: RequestContext,
    params: GetAttachmentRequest,
) -> Result<GetAttachmentResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx, &params.id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", params.id))?;

    if attachment.po.root_user_id != user_id {
        bail_err!(NotFound, "Attachment {} not found", params.id);
    }

    Ok(to_detail(&attachment))
}
