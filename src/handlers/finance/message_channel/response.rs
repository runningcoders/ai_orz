use crate::models::message_channel::{ChannelConfig, MessageChannel};
use crate::models::user_credential::UserCredential;
use common::api::{
    EmailChannelConfig, LarkChannelConfig, MessageChannelConfig, MessageChannelDetail,
    MessageChannelListItem, SlackChannelConfig, WebhookChannelConfig, WechatChannelConfig,
};

pub(super) fn to_list_item(
    channel: &MessageChannel,
    credentials: Option<&[UserCredential]>,
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
        config: Some(build_message_channel_config(
            &channel.po.config_json.0,
            credentials,
        )),
        last_pushed_at: channel.po.last_pushed_at,
        last_error: empty_to_none(channel.po.last_error.clone()),
        created_at: channel.po.created_at,
        updated_at: channel.po.updated_at,
    }
}

pub(super) fn to_detail(
    channel: &MessageChannel,
    credentials: Option<&[UserCredential]>,
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
        config: Some(build_message_channel_config(
            &channel.po.config_json.0,
            credentials,
        )),
        last_pushed_at: channel.po.last_pushed_at,
        last_error: empty_to_none(channel.po.last_error.clone()),
        created_by: channel.po.created_by.clone(),
        modified_by: channel.po.modified_by.clone(),
        created_at: channel.po.created_at,
        updated_at: channel.po.updated_at,
    }
}

fn build_message_channel_config(
    config: &ChannelConfig,
    credentials: Option<&[UserCredential]>,
) -> MessageChannelConfig {
    MessageChannelConfig {
        lark: build_lark_config(config, credentials),
        wechat: build_wechat_config(config),
        email: build_email_config(config),
        slack: build_slack_config(config),
        webhook: build_webhook_config(config),
    }
}

fn build_lark_config(
    config: &ChannelConfig,
    credentials: Option<&[UserCredential]>,
) -> Option<LarkChannelConfig> {
    let credential_id = non_empty_clone(config.lark_credential_id.as_deref());
    let credential_name = credential_name(config.lark_credential_id.as_deref(), credentials);
    let identity_mode = non_empty_clone(config.lark_identity_mode.as_deref());
    let open_id = non_empty_clone(config.lark_open_id.as_deref());
    let user_name = non_empty_clone(config.lark_user_name.as_deref());

    if credential_id.is_none()
        && credential_name.is_none()
        && identity_mode.is_none()
        && open_id.is_none()
        && user_name.is_none()
        && config.lark_listen_inbound.is_none()
    {
        return None;
    }

    Some(LarkChannelConfig {
        credential_id,
        credential_name,
        identity_mode,
        open_id,
        user_name,
        listen_inbound: config.lark_listen_inbound.unwrap_or(true),
    })
}

fn build_wechat_config(config: &ChannelConfig) -> Option<WechatChannelConfig> {
    let open_id = non_empty_clone(config.wechat_open_id.as_deref());
    if open_id.is_none() {
        return None;
    }
    Some(WechatChannelConfig { open_id })
}

fn build_email_config(config: &ChannelConfig) -> Option<EmailChannelConfig> {
    let smtp_host = non_empty_clone(config.email_smtp_host.as_deref());
    let smtp_port = config.email_smtp_port;
    let username = non_empty_clone(config.email_username.as_deref());
    let from_address = non_empty_clone(config.email_from_address.as_deref());
    let to_address = non_empty_clone(config.email_to_address.as_deref());

    if smtp_host.is_none()
        && smtp_port.is_none()
        && username.is_none()
        && from_address.is_none()
        && to_address.is_none()
    {
        return None;
    }

    Some(EmailChannelConfig {
        smtp_host,
        smtp_port,
        username,
        from_address,
        to_address,
    })
}

fn build_slack_config(config: &ChannelConfig) -> Option<SlackChannelConfig> {
    let channel_id = non_empty_clone(config.slack_channel_id.as_deref());
    if channel_id.is_none() {
        return None;
    }
    Some(SlackChannelConfig { channel_id })
}

fn build_webhook_config(config: &ChannelConfig) -> Option<WebhookChannelConfig> {
    let method = non_empty_clone(config.webhook_method.as_deref());
    let body_template = non_empty_clone(config.webhook_body_template.as_deref());

    if method.is_none() && body_template.is_none() {
        return None;
    }

    Some(WebhookChannelConfig {
        method,
        body_template,
    })
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

/// 凭证引用 ID 反查凭证名称（未传凭证列表或未找到时为 None）
fn credential_name(
    credential_id: Option<&str>,
    credentials: Option<&[UserCredential]>,
) -> Option<String> {
    let id = credential_id.filter(|v| !v.is_empty())?;
    credentials
        .and_then(|creds| creds.iter().find(|c| c.id() == id))
        .map(|c| c.name().to_string())
}

fn has_config_secret(config: &ChannelConfig) -> bool {
    has_value(&config.wechat_app_secret)
        || has_value(&config.email_password)
        || has_value(&config.slack_bot_token)
}
