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
///
/// `neural` 必需：附件引用出现在用户消息里，模型必须能读元信息与已记录摘要，
/// 否则「附件摘要」链路只有写侧、没有读侧。
#[register_handler_tool(
    id = "get_attachment",
    name = "Get Attachment Info",
    description = "Get an attachment's metadata by ID (file name, size, mime_type, purpose) without reading its content. Also returns the recorded AI summary: description (what the file contains), description_usage (inferred purpose) and provenance fields (description_source, description_model, description_message_id, described_at). An empty description means no summary has been recorded yet. Only your own attachments are visible — any other ID returns NotFound. Use get_attachment_content to read the text.",
    params = "common::api::GetAttachmentRequest",
    neural,
    tags = "attachment"
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
