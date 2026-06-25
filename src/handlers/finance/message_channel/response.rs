use crate::models::message_channel::{ChannelConfig, MessageChannel};
use common::api::{MessageChannelDetail, MessageChannelListItem};
use common::bail_err;

pub(super) fn to_list_item(channel: &MessageChannel) -> MessageChannelListItem {
    MessageChannelListItem {
        id: channel.po.id.clone(),
        org_id: channel.po.org_id.clone(),
        user_id: channel.po.user_id.clone(),
        agent_id: channel.po.agent_id.clone(),
        channel_type: channel.po.channel_type,
        channel_name: channel.po.channel_name.clone(),
        webhook_url: empty_to_none(channel.po.webhook_url.clone()),
        status: channel.po.status,
        has_access_token: has_value(&channel.po.access_token),
        has_secret: has_value(&channel.po.secret),
        has_config_secret: has_config_secret(&channel.po.config_json.0),
        last_pushed_at: channel.po.last_pushed_at,
        last_error: empty_to_none(channel.po.last_error.clone()),
        created_at: channel.po.created_at,
        updated_at: channel.po.updated_at,
    }
}

pub(super) fn to_detail(channel: &MessageChannel) -> MessageChannelDetail {
    MessageChannelDetail {
        id: channel.po.id.clone(),
        org_id: channel.po.org_id.clone(),
        user_id: channel.po.user_id.clone(),
        agent_id: channel.po.agent_id.clone(),
        channel_type: channel.po.channel_type,
        channel_name: channel.po.channel_name.clone(),
        webhook_url: empty_to_none(channel.po.webhook_url.clone()),
        status: channel.po.status,
        has_access_token: has_value(&channel.po.access_token),
        has_secret: has_value(&channel.po.secret),
        has_config_secret: has_config_secret(&channel.po.config_json.0),
        last_pushed_at: channel.po.last_pushed_at,
        last_error: empty_to_none(channel.po.last_error.clone()),
        created_by: channel.po.created_by.clone(),
        modified_by: channel.po.modified_by.clone(),
        created_at: channel.po.created_at,
        updated_at: channel.po.updated_at,
    }
}

fn has_value(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|v| !v.is_empty())
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

fn has_config_secret(config: &ChannelConfig) -> bool {
    has_value(&config.lark_app_secret)
        || has_value(&config.lark_encrypt_key)
        || has_value(&config.lark_verification_token)
        || has_value(&config.wechat_app_secret)
        || has_value(&config.email_password)
        || has_value(&config.slack_bot_token)
}
