//! Handler: POST /api/v1/message-channels/query - MessageChannel 通用查询接口
//!
//! 与 list_message_channels 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use crate::pkg::RequestContext;
use crate::service::dao::message_channel::MessageChannelQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MessageChannelListItem, MessageChannelQueryRequest, PagedResult};
use common::error::{Result, bail_err, err};

use super::response::to_list_item;

/// MessageChannel 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_message_channels",
    name = "Query Channels",
    description = "Filter message channels by exact fields: id, user_id, agent_id, channel_type, status_in, or only_enabled, with custom ordering and pagination. Use list_message_channels for simple browsing.",
    params = "common::api::MessageChannelQueryRequest",
    neural
)]
#[generate_http_handler]
pub async fn query_message_channels(
    ctx: RequestContext,
    params: MessageChannelQueryRequest,
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
            MessageChannelQuery {
                id: params.id,
                org_id: Some(org_id),
                user_id: Some(user_id.clone()),
                agent_id: params.agent_id,
                channel_type: params.channel_type,
                only_enabled: params.only_enabled.unwrap_or(false),
                status_in: params.status_in,
                order_by: params.order_by,
                pagination: params.pagination,
            },
        )
        .await?;

    // 凭证名称反查：查询默认按当前用户过滤，加载该用户凭证库即可
    let credentials = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .get_identity_credentials(ctx, &user_id)
        .await?
        .unwrap_or_default();

    Ok(page.map(|ch| to_list_item(&ch, Some(&credentials))))
}
