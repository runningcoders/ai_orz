use crate::models::message_channel::{ChannelConfig, MessageChannel};
use common::api::{MessageChannelDetail, MessageChannelListItem};
use common::models::UserIdentityCredentials;

pub(super) fn to_list_item(
    channel: &MessageChannel,
    credentials: Option<&UserIdentityCredentials>,
) -> MessageChannelListItem {
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
        lark_credential_id: non_empty_clone(channel.po.config_json.0.lark_credential_id.as_deref()),
        lark_credential_name: credential_name(
            channel.po.config_json.0.lark_credential_id.as_deref(),
            credentials,
        ),
        lark_identity_mode: non_empty_clone(channel.po.config_json.0.lark_identity_mode.as_deref()),
        lark_open_id: non_empty_clone(channel.po.config_json.0.lark_open_id.as_deref()),
        lark_user_name: non_empty_clone(channel.po.config_json.0.lark_user_name.as_deref()),
        lark_listen_inbound: channel.po.config_json.0.lark_listen_inbound.unwrap_or(true),
        last_pushed_at: channel.po.last_pushed_at,
        last_error: empty_to_none(channel.po.last_error.clone()),
        created_at: channel.po.created_at,
        updated_at: channel.po.updated_at,
    }
}

pub(super) fn to_detail(
    channel: &MessageChannel,
    credentials: Option<&UserIdentityCredentials>,
) -> MessageChannelDetail {
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
        lark_credential_id: non_empty_clone(channel.po.config_json.0.lark_credential_id.as_deref()),
        lark_credential_name: credential_name(
            channel.po.config_json.0.lark_credential_id.as_deref(),
            credentials,
        ),
        lark_identity_mode: non_empty_clone(channel.po.config_json.0.lark_identity_mode.as_deref()),
        lark_open_id: non_empty_clone(channel.po.config_json.0.lark_open_id.as_deref()),
        lark_user_name: non_empty_clone(channel.po.config_json.0.lark_user_name.as_deref()),
        lark_listen_inbound: channel.po.config_json.0.lark_listen_inbound.unwrap_or(true),
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

fn non_empty_clone(value: Option<&str>) -> Option<String> {
    value.filter(|v| !v.is_empty()).map(|v| v.to_string())
}

/// 凭证引用 ID 反查凭证名称（未传凭证库或未找到时为 None）
fn credential_name(
    credential_id: Option<&str>,
    library: Option<&UserIdentityCredentials>,
) -> Option<String> {
    let id = credential_id.filter(|v| !v.is_empty())?;
    library
        .and_then(|lib| lib.find_by_id(id))
        .map(|c| c.name.clone())
}

fn has_config_secret(config: &ChannelConfig) -> bool {
    has_value(&config.wechat_app_secret)
        || has_value(&config.email_password)
        || has_value(&config.slack_bot_token)
}
