//! Handler: GET /api/v1/attachments/{id}/content - 读取 Attachment UTF-8 文本内容

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AttachmentContentResponse, GetAttachmentContentRequest};
use common::error::Result;

use super::response::to_content_response;
use common::error::{bail_err, err};

/// 读取 Attachment UTF-8 文本内容
#[register_handler_tool(
    id = "get_attachment_content",
    name = "Read Attachment Content",
    description = "Read an attachment's UTF-8 text content together with its metadata. Only text files are supported (for example txt, md, json, yaml, csv, or source code) — binary attachments are rejected. Content is limited to 64 KB; use get_attachment for metadata only.",
    params = "common::api::GetAttachmentContentRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn get_attachment_content(
    ctx: RequestContext,
    params: GetAttachmentContentRequest,
) -> Result<AttachmentContentResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let content = domain()
        .attachment_manage()
        .get_attachment_text_content(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", params.id))?;

    Ok(to_content_response(&content))
}
