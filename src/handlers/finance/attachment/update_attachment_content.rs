//! Handler: PUT /api/v1/attachments/{id}/content - 全量替换 Attachment UTF-8 文本内容

use crate::models::attachment::TextContentUpdate;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AttachmentContentResponse, UpdateAttachmentContentRequest};
use common::error::Result;

use super::response::to_content_response;
use common::error::{bail_err, err};

/// 全量替换 Attachment UTF-8 文本内容
#[register_handler_tool(
    id = "update_attachment_content",
    name = "Update Attachment Content",
    description = "Fully replace the UTF-8 text content of an attachment by ID. Supports optimistic locking via expected_updated_at.",
    params = "common::api::UpdateAttachmentContentRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn update_attachment_content(
    ctx: RequestContext,
    params: UpdateAttachmentContentRequest,
) -> Result<AttachmentContentResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let content = domain()
        .attachment_manage()
        .update_attachment_text_content(
            ctx,
            &params.id,
            TextContentUpdate {
                content: params.content,
                expected_updated_at: params.expected_updated_at,
            },
        )
        .await?
        .ok_or_else(|| err!(NotFound, "Attachment {} not found", params.id))?;

    Ok(to_content_response(&content))
}
