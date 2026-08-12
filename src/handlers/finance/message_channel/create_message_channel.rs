//! Handler: POST /api/v1/message-channels - Create a new message channel

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateMessageChannelRequest, CreateMessageChannelResponse};
use common::models::UserIdentityCredentials;
use uuid::Uuid;

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// 飞书渠道凭证引用必填校验（纯函数，可单测）
///
/// 非飞书类型直接放行；飞书类型委托 common 凭证库的
/// `resolve_lark_credential_ref` 统一校验（存在 + kind=LarkApp）。
/// 归属校验天然成立：凭证库即按渠道归属用户加载。
pub fn validate_lark_credential_ref(
    channel_type: common::enums::ChannelType,
    lark_credential_id: Option<&str>,
    library: &UserIdentityCredentials,
) -> Result<()> {
    if !matches!(channel_type, common::enums::ChannelType::Lark) {
        return Ok(());
    }
    library.resolve_lark_credential_ref(lark_credential_id)?;
    Ok(())
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
    validate_lark_credential_ref(
        params.channel_type,
        params.lark_credential_id.as_deref(),
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
        ChannelConfig {
            lark_credential_id: params.lark_credential_id.clone(),
            lark_identity_mode: params.lark_identity_mode.clone(),
            wechat_app_id: params.wechat_app_id.clone(),
            wechat_app_secret: params.wechat_app_secret.clone(),
            wechat_open_id: params.wechat_open_id.clone(),
            email_smtp_host: params.email_smtp_host.clone(),
            email_smtp_port: params.email_smtp_port,
            email_username: params.email_username.clone(),
            email_password: params.email_password.clone(),
            email_from_address: params.email_from_address.clone(),
            email_to_address: params.email_to_address.clone(),
            slack_bot_token: params.slack_bot_token.clone(),
            slack_channel_id: params.slack_channel_id.clone(),
            webhook_method: params.webhook_method.clone(),
            webhook_headers: None,
            webhook_body_template: params.webhook_body_template.clone(),
            lark_open_id: params.lark_open_id.clone(),
            lark_user_name: params.lark_user_name.clone(),
            lark_listen_inbound: params.lark_listen_inbound,
            extra: None,
        },
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
    use common::enums::ChannelType;
    use common::models::{CredentialDetail, CredentialKind, UserIdentityCredential};

    fn lark_library(credential_id: &str) -> UserIdentityCredentials {
        UserIdentityCredentials {
            items: vec![UserIdentityCredential {
                id: credential_id.to_string(),
                kind: CredentialKind::LarkApp,
                name: "测试凭证".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                detail: CredentialDetail::LarkApp {
                    app_id: "cli_x".to_string(),
                    app_secret: "enc:v1:secret".to_string(),
                    encrypt_key: None,
                    verification_token: None,
                },
            }],
            ..Default::default()
        }
    }

    #[test]
    fn lark_credential_ref_required_for_lark_type() {
        let library = lark_library("cred-1");
        assert!(validate_lark_credential_ref(ChannelType::Lark, None, &library).is_err());
        assert!(validate_lark_credential_ref(ChannelType::Lark, Some("  "), &library).is_err());
        // 引用不存在的凭证
        assert!(
            validate_lark_credential_ref(ChannelType::Lark, Some("missing"), &library).is_err()
        );
        // 引用存在的 LarkApp 凭证
        assert!(validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &library).is_ok());
    }

    #[test]
    fn empty_library_rejects_any_ref() {
        let library = UserIdentityCredentials::default();
        assert!(validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &library).is_err());
    }

    #[test]
    fn non_lark_type_skips_credential_validation() {
        let library = UserIdentityCredentials::default();
        assert!(validate_lark_credential_ref(ChannelType::Webhook, None, &library).is_ok());
        assert!(validate_lark_credential_ref(ChannelType::Email, None, &library).is_ok());
    }
}
