//! Handler: POST /api/v1/attachments/description - 写入附件摘要

use crate::models::attachment::AttachmentDescriptionUpdate;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SaveAttachmentDescriptionRequest, SaveAttachmentDescriptionResponse};
use common::error::Result;
use common::error::bail_err;

use super::response::to_detail;

/// 写入附件摘要元信息
///
/// `neural` 必需：Awaken 会为当前消息的附件追加「写摘要」引导，若工具不可达，
/// 引导会变成模型做不到的空话（与消息类工具同源的静默失效）。
#[register_handler_tool(
    id = "save_attachment_description",
    name = "Save Attachment Description",
    description = "Record what you actually saw in an attachment, so its summary and inferred purpose are stored for search and reuse later. Call it once per attachment after you have identified the content: content = an objective summary of what the file contains; usage = what the user wants to do with it based on their message (optional); attachment_id = the attachment ID; message_id = the current message ID (optional). Calling again overwrites the previous summary. Only your own attachments can be described.",
    params = "common::api::SaveAttachmentDescriptionRequest",
    neural,
    tags = "attachment"
)]
#[generate_http_handler]
pub async fn save_attachment_description(
    ctx: RequestContext,
    params: SaveAttachmentDescriptionRequest,
) -> Result<SaveAttachmentDescriptionResponse> {
    if ctx.uid().is_empty() && ctx.caller_type() != common::enums::CallerType::System {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let update = AttachmentDescriptionUpdate {
        description: params.content,
        usage: params.usage.unwrap_or_default(),
        source: "agent_tool".to_string(),
        model: ctx.model_name.clone().unwrap_or_default(),
        message_id: params.message_id.unwrap_or_default(),
        described_at: common::constants::utils::current_timestamp_ms(),
    };

    let attachment = domain()
        .attachment_manage()
        .describe_attachment(ctx, &params.attachment_id, update)
        .await?;

    Ok(to_detail(&attachment))
}
