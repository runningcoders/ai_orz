//! Handler: PUT /api/v1/message-channels/{id} - Update message channel configuration

use crate::models::message_channel::ChannelConfig;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    CreateMessageChannelConfig, UpdateMessageChannelRequest, UpdateMessageChannelResponse,
};

use super::create_message_channel::validate_lark_credential_ref;
use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// Update an existing message channel configuration (name, credentials, settings, etc.)
#[register_handler_tool(
    id = "update_message_channel",
    name = "Update Channel Config",
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

    merge_channel_config(&mut channel.po.config_json.0, &params.config);

    channel.po.modified_by = current_user_id;

    // 飞书渠道更新后凭证引用必须有效（按渠道归属用户加载凭证库，含归属校验）
    let credentials = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .get_identity_credentials(ctx.clone(), &channel.po.user_id)
        .await?
        .unwrap_or_default();
    validate_lark_credential_ref(
        channel.po.channel_type,
        channel.po.config_json.0.lark_credential_id.as_deref(),
        &credentials,
    )?;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(to_detail(&channel, Some(&credentials)))
}

/// Merge CreateMessageChannelConfig into ChannelConfig (only updates Some fields)
fn merge_channel_config(target: &mut ChannelConfig, source: &Option<CreateMessageChannelConfig>) {
    let Some(config) = source else {
        return;
    };

    if let Some(lark) = &config.lark {
        if let Some(v) = &lark.credential_id {
            target.lark_credential_id = Some(v.clone());
        }
        if let Some(v) = &lark.identity_mode {
            target.lark_identity_mode = Some(v.clone());
        }
        if let Some(v) = &lark.open_id {
            target.lark_open_id = Some(v.clone());
        }
        if let Some(v) = &lark.user_name {
            target.lark_user_name = Some(v.clone());
        }
        if let Some(v) = lark.listen_inbound {
            target.lark_listen_inbound = Some(v);
        }
    }

    if let Some(wechat) = &config.wechat {
        if let Some(v) = &wechat.app_id {
            target.wechat_app_id = Some(v.clone());
        }
        if let Some(v) = &wechat.app_secret {
            target.wechat_app_secret = Some(v.clone());
        }
        if let Some(v) = &wechat.open_id {
            target.wechat_open_id = Some(v.clone());
        }
    }

    if let Some(email) = &config.email {
        if let Some(v) = &email.smtp_host {
            target.email_smtp_host = Some(v.clone());
        }
        if let Some(v) = email.smtp_port {
            target.email_smtp_port = Some(v);
        }
        if let Some(v) = &email.username {
            target.email_username = Some(v.clone());
        }
        if let Some(v) = &email.password {
            target.email_password = Some(v.clone());
        }
        if let Some(v) = &email.from_address {
            target.email_from_address = Some(v.clone());
        }
        if let Some(v) = &email.to_address {
            target.email_to_address = Some(v.clone());
        }
    }

    if let Some(slack) = &config.slack {
        if let Some(v) = &slack.bot_token {
            target.slack_bot_token = Some(v.clone());
        }
        if let Some(v) = &slack.channel_id {
            target.slack_channel_id = Some(v.clone());
        }
    }

    if let Some(webhook) = &config.webhook {
        if let Some(v) = &webhook.method {
            target.webhook_method = Some(v.clone());
        }
        if let Some(v) = &webhook.body_template {
            target.webhook_body_template = Some(v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::api::{
        CreateEmailChannelConfig, CreateLarkChannelConfig, CreateMessageChannelConfig,
    };

    #[test]
    fn merge_config_noop_on_none() {
        let mut config = ChannelConfig::default();
        merge_channel_config(&mut config, &None);
        assert!(config.lark_credential_id.is_none());
    }

    #[test]
    fn merge_config_updates_lark_fields() {
        let mut config = ChannelConfig::default();
        let source = Some(CreateMessageChannelConfig {
            lark: Some(CreateLarkChannelConfig {
                credential_id: Some("cred-1".to_string()),
                identity_mode: Some("bot".to_string()),
                open_id: Some("ou_xxx".to_string()),
                user_name: Some("Test".to_string()),
                listen_inbound: Some(true),
            }),
            wechat: None,
            email: None,
            slack: None,
            webhook: None,
        });
        merge_channel_config(&mut config, &source);
        assert_eq!(config.lark_credential_id.as_deref(), Some("cred-1"));
        assert_eq!(config.lark_identity_mode.as_deref(), Some("bot"));
        assert_eq!(config.lark_open_id.as_deref(), Some("ou_xxx"));
        assert_eq!(config.lark_user_name.as_deref(), Some("Test"));
        assert_eq!(config.lark_listen_inbound, Some(true));
    }

    #[test]
    fn merge_config_updates_email_fields() {
        let mut config = ChannelConfig::default();
        let source = Some(CreateMessageChannelConfig {
            lark: None,
            wechat: None,
            email: Some(CreateEmailChannelConfig {
                smtp_host: Some("smtp.test.com".to_string()),
                smtp_port: Some(587),
                username: Some("user".to_string()),
                password: Some("pass".to_string()),
                from_address: Some("from@test.com".to_string()),
                to_address: Some("to@test.com".to_string()),
            }),
            slack: None,
            webhook: None,
        });
        merge_channel_config(&mut config, &source);
        assert_eq!(config.email_smtp_host.as_deref(), Some("smtp.test.com"));
        assert_eq!(config.email_smtp_port, Some(587));
        assert_eq!(config.email_username.as_deref(), Some("user"));
        assert_eq!(config.email_password.as_deref(), Some("pass"));
        assert_eq!(config.email_from_address.as_deref(), Some("from@test.com"));
        assert_eq!(config.email_to_address.as_deref(), Some("to@test.com"));
    }

    #[test]
    fn merge_config_preserves_existing_when_not_specified() {
        let mut config = ChannelConfig {
            lark_credential_id: Some("old-cred".to_string()),
            email_smtp_host: Some("old.host".to_string()),
            ..Default::default()
        };
        let source = Some(CreateMessageChannelConfig {
            lark: Some(CreateLarkChannelConfig {
                credential_id: None,
                identity_mode: None,
                open_id: None,
                user_name: None,
                listen_inbound: None,
            }),
            wechat: None,
            email: None,
            slack: None,
            webhook: None,
        });
        merge_channel_config(&mut config, &source);
        // Existing preserved since no Some values were provided
        assert_eq!(config.lark_credential_id.as_deref(), Some("old-cred"));
        assert_eq!(config.email_smtp_host.as_deref(), Some("old.host"));
    }
}
