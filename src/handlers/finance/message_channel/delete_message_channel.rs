//! Handler: DELETE /api/v1/message-channels/{id} - Delete a message channel

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteMessageChannelRequest, EmptyResponse};
use common::error::{Result, err, bail_err};

/// Delete an existing message channel (soft delete)
#[register_handler_tool(
    id = "delete_message_channel",
    name = "delete_message_channel",
    description = "Delete an existing message channel (soft delete)",
    params = "common::api::DeleteMessageChannelRequest"
)]
#[generate_http_handler]
pub async fn delete_message_channel(
    ctx: RequestContext,
    params: DeleteMessageChannelRequest,
) -> Result<EmptyResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "MessageChannel {} not found", params.id))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        bail_err!(NotFound, "MessageChannel {} not found", params.id);
    }

    domain()
        .message_channel_manage()
        .delete_message_channel(ctx, &channel)
        .await?;

    Ok(EmptyResponse {})
}