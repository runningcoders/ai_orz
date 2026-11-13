//! Handler: GET /api/v1/finance/message-channels/{id}/conversations
//!
//! 分页返回某渠道自动绑定的外部会话（入站路由的「应用 + 租户 + 会话」记录），
//! 用于前端在渠道详情页查看 / 核对该渠道当前服务的私聊与群会话。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    ListMessageChannelConversationsRequest, ListMessageChannelConversationsResponse,
    MessageChannelConversationItem, PaginationParams,
};
use common::error::{Result, bail_err, err};

/// 分页查询渠道绑定的外部会话列表
#[register_handler_tool(
    id = "list_message_channel_conversations",
    name = "List Channel Conversations",
    description = "List the external conversations (p2p chats / group chats) automatically bound to one message channel, paginated by limit/offset. Only the channel owner in the current organization can read it.",
    params = "common::api::ListMessageChannelConversationsRequest",
    tags = "admin"
)]
#[generate_http_handler]
pub async fn list_message_channel_conversations(
    ctx: RequestContext,
    params: ListMessageChannelConversationsRequest,
) -> Result<ListMessageChannelConversationsResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 归属校验：会话绑定依附渠道，先确认渠道属于当前组织 + 当前用户
    let channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "MessageChannel {} not found", params.id))?;
    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        bail_err!(NotFound, "MessageChannel {} not found", params.id);
    }

    let page = domain()
        .message_channel_manage()
        .list_channel_conversations(
            ctx,
            &params.id,
            PaginationParams {
                limit: params.limit,
                offset: params.offset,
            },
        )
        .await?;

    Ok(page.map(|conversation| MessageChannelConversationItem {
        id: conversation.id().to_string(),
        channel_id: conversation.channel_id().to_string(),
        channel_type: conversation.channel_type(),
        app_id: conversation.app_id().to_string(),
        tenant_key: conversation.tenant_key().to_string(),
        chat_id: conversation.chat_id().to_string(),
        created_at: conversation.created_at(),
        updated_at: conversation.updated_at(),
    }))
}
