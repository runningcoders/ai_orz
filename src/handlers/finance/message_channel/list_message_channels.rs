//! Handler: GET /api/v1/message-channels - List message channels with filtering

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    ListMessageChannelsRequest, ListMessageChannelsResponse, MessageChannelListItem,
};

use super::response::to_list_item;
use common::error::{Result, err, bail_err};

/// List message channels with optional filtering by user, agent, channel type, enabled status
#[register_handler_tool(
    id = "list_message_channels",
    name = "list_message_channels",
    description = "List message channels with optional filtering by user, agent, channel type, enabled status",
    params = "common::api::ListMessageChannelsRequest"
)]
#[generate_http_handler]
pub async fn list_message_channels(
    ctx: RequestContext,
    params: ListMessageChannelsRequest,
) -> Result<ListMessageChannelsResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = params.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let channels = domain()
        .message_channel_manage()
        .query_channels(
            ctx.clone(),
            crate::service::dao::message_channel::MessageChannelQuery {
                org_id: Some(org_id),
                user_id: Some(user_id),
                agent_id: params.agent_id.clone(),
                channel_type: params.channel_type,
                only_enabled: params.only_enabled.unwrap_or(false),
                limit: params.limit,
                offset: params.offset,
                ..Default::default()
            },
        )
        .await?;

    let total = channels.len();
    let channels: Vec<MessageChannelListItem> = channels.iter().map(to_list_item).collect();
    Ok(ListMessageChannelsResponse {
        channels,
        total: total as usize,
    })
}