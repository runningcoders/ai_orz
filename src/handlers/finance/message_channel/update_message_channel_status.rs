//! Handler: PUT /api/v1/message-channels/{id}/status - Update message channel status (active/disabled)

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateMessageChannelStatusRequest, UpdateMessageChannelStatusResponse, MessageChannelDetail};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// Update the status of a message channel (enable/disable it)
#[register_handler_tool(
    id = "update_message_channel_status",
    name = "update_message_channel_status",
    description = "Update the status of a message channel (enable/disable it)",
    params = "common::api::UpdateMessageChannelStatusRequest",
)]
#[generate_http_handler]
pub async fn update_message_channel_status(
    ctx: RequestContext,
    params: UpdateMessageChannelStatusRequest,
) -> Result<UpdateMessageChannelStatusResponse, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", params.id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        return Err(AppError::NotFound(format!(
            "MessageChannel {} not found",
            params.id
        )));
    }

    channel
        .transition_status(params.status, user_id)
        .map_err(AppError::BadRequest)?;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(to_detail(&channel))
}