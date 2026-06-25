//! Handler: PUT /api/v1/message-channels/{id} - Update message channel configuration

use common::bail_err;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    MessageChannelDetail, UpdateMessageChannelRequest, UpdateMessageChannelResponse,
};

use super::response::to_detail;
use common::error::Result;
use common::err;

/// Update an existing message channel configuration (name, credentials, settings, etc.)
#[register_handler_tool(
    id = "update_message_channel",
    name = "update_message_channel",
    description = "Update an existing message channel configuration (name, credentials, settings, etc.)",
    params = "common::api::UpdateMessageChannelRequest"
)]
#[generate_http_handler]
pub async fn update_message_channel(
    ctx: RequestContext,
    params: UpdateMessageChannelRequest,
) -> Result<UpdateMessageChannelResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let mut channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "MessageChannel {} not found", params.id))?;

    if channel.po.org_id != org_id || channel.po.user_id != current_user_id {
        bail_err!(NotFound, "MessageChannel {} not found", params.id);
    }

    if let Some(user_id) = params.user_id {
        channel.po.user_id = user_id;
    }
    if let Some(agent_id) = params.agent_id {
        channel.po.agent_id = Some(agent_id);
    }
    if let Some(channel_type) = params.channel_type {
        channel.po.channel_type = channel_type;
    }
    if let Some(channel_name) = params.channel_name {
        channel.po.channel_name = channel_name;
    }
    if let Some(webhook_url) = params.webhook_url {
        channel.po.webhook_url = Some(webhook_url);
    }
    if let Some(access_token) = params.access_token {
        channel.po.access_token = Some(access_token);
    }
    if let Some(secret) = params.secret {
        channel.po.secret = Some(secret);
    }

    let config = &mut channel.po.config_json.0;
    if let Some(value) = params.lark_app_id {
        config.lark_app_id = Some(value);
    }
    if let Some(value) = params.lark_app_secret {
        config.lark_app_secret = Some(value);
    }
    if let Some(value) = params.lark_encrypt_key {
        config.lark_encrypt_key = Some(value);
    }
    if let Some(value) = params.lark_verification_token {
        config.lark_verification_token = Some(value);
    }
    if let Some(value) = params.wechat_app_id {
        config.wechat_app_id = Some(value);
    }
    if let Some(value) = params.wechat_app_secret {
        config.wechat_app_secret = Some(value);
    }
    if let Some(value) = params.wechat_open_id {
        config.wechat_open_id = Some(value);
    }
    if let Some(value) = params.email_smtp_host {
        config.email_smtp_host = Some(value);
    }
    if let Some(value) = params.email_smtp_port {
        config.email_smtp_port = Some(value);
    }
    if let Some(value) = params.email_username {
        config.email_username = Some(value);
    }
    if let Some(value) = params.email_password {
        config.email_password = Some(value);
    }
    if let Some(value) = params.email_from_address {
        config.email_from_address = Some(value);
    }
    if let Some(value) = params.email_to_address {
        config.email_to_address = Some(value);
    }
    if let Some(value) = params.slack_bot_token {
        config.slack_bot_token = Some(value);
    }
    if let Some(value) = params.slack_channel_id {
        config.slack_channel_id = Some(value);
    }
    if let Some(value) = params.webhook_method {
        config.webhook_method = Some(value);
    }
    if let Some(value) = params.webhook_body_template {
        config.webhook_body_template = Some(value);
    }

    channel.po.modified_by = current_user_id;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(to_detail(&channel))
}