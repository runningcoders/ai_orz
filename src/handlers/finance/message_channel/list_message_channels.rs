//! Handler: GET /api/v1/message-channels - List message channels with filtering

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListMessageChannelsRequest, MessageChannelListItem, PagedResult};

use super::response::to_list_item;
use common::error::{Result, bail_err, err};

/// List message channels with optional filtering by user, agent, channel type, enabled status
#[register_handler_tool(
    id = "list_message_channels",
    name = "List All Channels",
    description = "List message channels with optional filtering by user, agent, channel type, enabled status",
    params = "common::api::ListMessageChannelsRequest"
)]
#[generate_http_handler]
pub async fn list_message_channels(
    ctx: RequestContext,
    params: ListMessageChannelsRequest,
) -> Result<PagedResult<MessageChannelListItem>> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = params.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let page = domain()
        .message_channel_manage()
        .query_channels(
            ctx.clone(),
            crate::service::dao::message_channel::MessageChannelQuery {
                org_id: Some(org_id),
                user_id: Some(user_id.clone()),
                agent_id: params.agent_id.clone(),
                channel_type: params.channel_type,
                only_enabled: params.only_enabled.unwrap_or(false),
                pagination: common::api::PaginationParams {
                    limit: params.limit,
                    offset: params.offset,
                },
                ..Default::default()
            },
        )
        .await?;

    // 凭证名称反查：列表默认按当前用户过滤，加载该用户凭证库即可
    let credentials = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .get_identity_credentials(ctx, &user_id)
        .await?
        .unwrap_or_default();

    Ok(page.map(|ch| to_list_item(&ch, Some(&credentials))))
}
