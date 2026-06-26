//! Handler: PUT /api/v1/message-channels/{id}/status - Update message channel status (active/disabled)

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    MessageChannelDetail, UpdateMessageChannelStatusRequest, UpdateMessageChannelStatusResponse,
};

use super::response::to_detail;
use common::error::{Result, err, bail_err};

/// Update the status of a message channel (enable/disable it)
#[register_handler_tool(
    id = "update_message_channel_status",
    name = "update_message_channel_status",
    description = "Update the status of a message channel (enable/disable it)",
    params = "common::api::UpdateMessageChannelStatusRequest"
)]
#[generate_http_handler]
pub async fn update_message_channel_status(
    ctx: RequestContext,
    params: UpdateMessageChannelStatusRequest,
) -> Result<UpdateMessageChannelStatusResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let mut channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "MessageChannel {} not found", params.id))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        bail_err!(NotFound, "MessageChannel {} not found", params.id);
    }

    channel
        .transition_status(params.status, user_id)
        .map_err(|e| err!(InvalidRequest, "{}", e))?;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(to_detail(&channel))
}