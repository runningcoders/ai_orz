//! Handler: POST /api/v1/message-channels - Create a new message channel

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateMessageChannelRequest, CreateMessageChannelResponse};
use uuid::Uuid;

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::models::user_credential::UserCredential;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, bail_err, err};
use common::models::CredentialKind;

/// 飞书渠道凭证引用必填校验（纯函数，可单测）
///
/// 非飞书类型直接放行；飞书类型校验引用 ID 非空 +
/// 凭证存在且 kind=LarkApp。归属校验天然成立：
/// 凭证列表即按渠道归属用户加载。
pub fn validate_lark_credential_ref(
    channel_type: common::enums::ChannelType,
    lark_credential_id: Option<&str>,
    credentials: &[UserCredential],
) -> Result<()> {
    if !matches!(channel_type, common::enums::ChannelType::Lark) {
        return Ok(());
    }
    let credential_id = lark_credential_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "飞书渠道必须引用用户级应用凭证（lark_credential_id）"
            )
        })?;
    let credential = credentials
        .iter()
        .find(|c| c.id() == credential_id)
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "引用的飞书凭证不存在 credential_id={}",
                credential_id
            )
        })?;
    if credential.kind() != CredentialKind::LarkApp {
        bail_err!(
            InvalidRequest,
            "引用的凭证不是飞书应用凭证 credential_id={}",
            credential_id
        );
    }
    Ok(())
}

/// Extract ChannelConfig from CreateMessageChannelRequest
fn extract_channel_config(req: &CreateMessageChannelRequest) -> ChannelConfig {
    let mut config = ChannelConfig::default();

    if let Some(channel_config) = &req.config {
        if let Some(lark) = &channel_config.lark {
            config.lark_credential_id = lark.credential_id.clone();
            config.lark_identity_mode = lark.identity_mode.clone();
            config.lark_open_id = lark.open_id.clone();
            config.lark_user_name = lark.user_name.clone();
            config.lark_listen_inbound = lark.listen_inbound;
        }
        if let Some(wechat) = &channel_config.wechat {
            config.wechat_app_id = wechat.app_id.clone();
            config.wechat_app_secret = wechat.app_secret.clone();
            config.wechat_open_id = wechat.open_id.clone();
        }
        if let Some(email) = &channel_config.email {
            config.email_smtp_host = email.smtp_host.clone();
            config.email_smtp_port = email.smtp_port;
            config.email_username = email.username.clone();
            config.email_password = email.password.clone();
            config.email_from_address = email.from_address.clone();
            config.email_to_address = email.to_address.clone();
        }
        if let Some(slack) = &channel_config.slack {
            config.slack_bot_token = slack.bot_token.clone();
            config.slack_channel_id = slack.channel_id.clone();
        }
        if let Some(webhook) = &channel_config.webhook {
            config.webhook_method = webhook.method.clone();
            config.webhook_body_template = webhook.body_template.clone();
        }
    }

    config
}

/// Create a new message channel for sending notifications to external services/users
#[register_handler_tool(
    id = "create_message_channel",
    name = "create_message_channel",
    description = "Create a new message channel for sending notifications to external services/users",
    params = "common::api::CreateMessageChannelRequest"
)]
#[generate_http_handler]
pub async fn create_message_channel(
    ctx: RequestContext,
    params: CreateMessageChannelRequest,
) -> Result<CreateMessageChannelResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = params.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 飞书渠道必须引用用户级应用凭证（凭证归属校验 = 按渠道用户加载凭证库）
    let credentials = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .get_identity_credentials(ctx.clone(), &user_id)
        .await?
        .unwrap_or_default();

    let channel_config = extract_channel_config(&params);
    validate_lark_credential_ref(
        params.channel_type,
        channel_config.lark_credential_id.as_deref(),
        &credentials,
    )?;

    let channel_po = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        org_id,
        user_id,
        params.agent_id.clone(),
        params.channel_type,
        params.channel_name.clone(),
        params.webhook_url.clone(),
        params.access_token.clone(),
        params.secret.clone(),
        channel_config,
        ctx.uid(),
    );
    let channel = MessageChannel::from_po(channel_po);

    domain()
        .message_channel_manage()
        .create_message_channel(ctx.clone(), &channel)
        .await?;

    Ok(to_detail(&channel, Some(&credentials)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user_credential::UserCredentialPo;
    use common::api::CreateMessageChannelConfig;
    use common::enums::ChannelType;
    use common::models::{CredentialDetail, CredentialVisibility};

    fn lark_credentials(credential_id: &str) -> Vec<UserCredential> {
        vec![UserCredential::from_po(UserCredentialPo::new(
            credential_id.to_string(),
            "org-1".to_string(),
            "user-1".to_string(),
            CredentialKind::LarkApp,
            "测试凭证".to_string(),
            CredentialDetail::LarkApp {
                app_id: "cli_x".to_string(),
                app_secret: "enc:v1:secret".to_string(),
                encrypt_key: None,
                verification_token: None,
            },
            CredentialVisibility::Private,
            "user-1".to_string(),
        ))]
    }

    #[test]
    fn lark_credential_ref_required_for_lark_type() {
        let credentials = lark_credentials("cred-1");
        assert!(validate_lark_credential_ref(ChannelType::Lark, None, &credentials).is_err());
        assert!(validate_lark_credential_ref(ChannelType::Lark, Some("  "), &credentials).is_err());
        // 引用不存在的凭证
        assert!(
            validate_lark_credential_ref(ChannelType::Lark, Some("missing"), &credentials).is_err()
        );
        // 引用存在的 LarkApp 凭证
        assert!(
            validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &credentials).is_ok()
        );
    }

    #[test]
    fn empty_credentials_rejects_any_ref() {
        let credentials: Vec<UserCredential> = Vec::new();
        assert!(
            validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &credentials).is_err()
        );
    }

    #[test]
    fn non_lark_type_skips_credential_validation() {
        let credentials: Vec<UserCredential> = Vec::new();
        assert!(validate_lark_credential_ref(ChannelType::Webhook, None, &credentials).is_ok());
        assert!(validate_lark_credential_ref(ChannelType::Email, None, &credentials).is_ok());
    }

    #[test]
    fn extract_config_handles_none() {
        let req = CreateMessageChannelRequest {
            user_id: None,
            agent_id: None,
            channel_type: ChannelType::Lark,
            channel_name: "test".to_string(),
            webhook_url: None,
            access_token: None,
            secret: None,
            config: None,
        };
        let config = extract_channel_config(&req);
        assert!(config.lark_credential_id.is_none());
        assert!(config.wechat_app_id.is_none());
        assert!(config.email_smtp_host.is_none());
    }

    #[test]
    fn extract_config_extracts_lark_fields() {
        let req = CreateMessageChannelRequest {
            user_id: None,
            agent_id: None,
            channel_type: ChannelType::Lark,
            channel_name: "test".to_string(),
            webhook_url: None,
            access_token: None,
            secret: None,
            config: Some(CreateMessageChannelConfig {
                lark: Some(common::api::CreateLarkChannelConfig {
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
            }),
        };
        let config = extract_channel_config(&req);
        assert_eq!(config.lark_credential_id.as_deref(), Some("cred-1"));
        assert_eq!(config.lark_identity_mode.as_deref(), Some("bot"));
        assert_eq!(config.lark_open_id.as_deref(), Some("ou_xxx"));
        assert_eq!(config.lark_user_name.as_deref(), Some("Test"));
        assert_eq!(config.lark_listen_inbound, Some(true));
    }
}
