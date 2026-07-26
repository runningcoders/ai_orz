//! Handler: DELETE /api/v1/attachments/{id} - 删除 Attachment

use common::error::Result;
use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteAttachmentRequest, EmptyResponse};
use common::error::{err, bail_err};

/// 删除 Attachment
#[register_handler_tool(
    id = "delete_attachment",
    name = "delete_attachment",
    description = "Delete an attachment by ID. Only the owner (root_user_id) can delete. Soft delete, data preserved for audit.",
    params = "common::api::DeleteAttachmentRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn delete_attachment(
    ctx: RequestContext,
    params: DeleteAttachmentRequest,
) -> Result<EmptyResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .get_attachment(ctx.clone(), &params.id, AttachmentGetOptions::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", params.id))?;

    if attachment.po.root_user_id != user_id {
        bail_err!(NotFound, "Attachment {} not found", params.id);
    }

    domain()
        .attachment_manage()
        .delete_attachment(ctx, &params.id)
        .await?;

    Ok(EmptyResponse {})
}
