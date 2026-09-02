//! Handler: POST /api/v1/attachments/text - 创建小型 UTF-8 文本 Attachment

use crate::models::attachment::TextAttachmentCreate;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateTextAttachmentRequest, CreateTextAttachmentResponse};
use common::error::Result;
use common::error::bail_err;

use super::response::to_detail;

/// 创建小型 UTF-8 文本 Attachment
#[register_handler_tool(
    id = "create_text_attachment",
    name = "Create Text Attachment",
    description = "Create a small UTF-8 text attachment from inline content, with a file name, optional text mime_type, and a purpose (skill, message, artifact, or tool_result). Returns the attachment detail, whose ID can be passed to message or skill APIs to attach the file. Content must be plain text under 64 KB.",
    params = "common::api::CreateTextAttachmentRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn create_text_attachment(
    ctx: RequestContext,
    params: CreateTextAttachmentRequest,
) -> Result<CreateTextAttachmentResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let attachment = domain()
        .attachment_manage()
        .create_text_attachment(
            ctx,
            TextAttachmentCreate {
                file_name: params.file_name,
                content: params.content,
                mime_type: params.mime_type,
                purpose: params.purpose,
            },
        )
        .await?;

    Ok(to_detail(&attachment))
}
