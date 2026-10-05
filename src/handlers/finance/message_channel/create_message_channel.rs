//! Handler: POST /api/v1/message-channels - Create a new message channel

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateMessageChannelRequest, CreateMessageChannelResponse};
use uuid::Uuid;

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::models::user_credential::UserCredential;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::constants::sentinel::fold_clear_sentinel;
use common::error::{Result, bail_err, err};
use common::models::CredentialKind;

/// 渠道凭证引用必填校验（纯函数，可单测）
///
/// 仅对 `expected_channel` 类型生效：校验引用 ID 非空 + 凭证存在且 kind 匹配。
/// 归属校验天然成立：凭证列表即按渠道归属用户加载。
///
/// 飞书（LarkApp）与微信 iLink（WechatIlink）共用此判据——两者都是
/// 「渠道只存引用，长期凭证走凭据表」的引用模式。
pub fn validate_channel_credential_ref(
    channel_type: common::enums::ChannelType,
    credential_id: Option<&str>,
    expected_channel: common::enums::ChannelType,
    expected_kind: CredentialKind,
    channel_label: &str,
    credentials: &[UserCredential],
) -> Result<()> {
    if channel_type != expected_channel {
        return Ok(());
    }
    let credential_id = credential_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "{}渠道必须引用用户级 {} 凭证（credential_id）",
                channel_label,
                expected_kind.as_str()
            )
        })?;
    let credential = credentials
        .iter()
        .find(|c| c.id() == credential_id)
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "引用的 {} 凭证不存在 credential_id={}",
                expected_kind.as_str(),
                credential_id
            )
        })?;
    if credential.kind() != expected_kind {
        bail_err!(
            InvalidRequest,
            "引用的凭证不是 {} 凭证 credential_id={}",
            expected_kind.as_str(),
            credential_id
        );
    }
    Ok(())
}

/// 飞书渠道凭证引用校验（LarkApp）
pub fn validate_lark_credential_ref(
    channel_type: common::enums::ChannelType,
    lark_credential_id: Option<&str>,
    credentials: &[UserCredential],
) -> Result<()> {
    validate_channel_credential_ref(
        channel_type,
        lark_credential_id,
        common::enums::ChannelType::Lark,
        CredentialKind::LarkApp,
        "飞书",
        credentials,
    )
}

/// 微信渠道凭证引用校验（WechatIlink）
pub fn validate_wechat_credential_ref(
    channel_type: common::enums::ChannelType,
    wechat_credential_id: Option<&str>,
    credentials: &[UserCredential],
) -> Result<()> {
    validate_channel_credential_ref(
        channel_type,
        wechat_credential_id,
        common::enums::ChannelType::Wechat,
        CredentialKind::WechatIlink,
        "微信",
        credentials,
    )
}

/// 邮箱渠道凭证引用校验（EmailBot）
pub fn validate_email_credential_ref(
    channel_type: common::enums::ChannelType,
    email_credential_id: Option<&str>,
    credentials: &[UserCredential],
) -> Result<()> {
    validate_channel_credential_ref(
        channel_type,
        email_credential_id,
        common::enums::ChannelType::Email,
        CredentialKind::EmailBot,
        "邮箱",
        credentials,
    )
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
            config.wechat_credential_id = wechat.credential_id.clone();
            config.wechat_peer_id = wechat.peer_id.clone();
            config.wechat_listen_inbound = wechat.listen_inbound;
        }
        if let Some(email) = &channel_config.email {
            config.email_credential_id = email.credential_id.clone();
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
    name = "Add Message Channel",
    description = "Register an outbound notification channel (Lark, WeChat, Email, Slack, or Webhook) that messages can be delivered through, optionally bound to an agent. Returns the channel detail. Lark channels must reference an existing LarkApp credential via lark_credential_id; WeChat channels must reference a WechatIlink credential; Email channels must reference an EmailBot credential plus a recipient to_address.",
    params = "common::api::CreateMessageChannelRequest",
    tags = "admin"
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
    validate_wechat_credential_ref(
        params.channel_type,
        channel_config.wechat_credential_id.as_deref(),
        &credentials,
    )?;
    validate_email_credential_ref(
        params.channel_type,
        channel_config.email_credential_id.as_deref(),
        &credentials,
    )?;

    // 哨兵只在部分更新协议中有意义；创建路径防御性折叠，保证哨兵永不落库
    let agent_id = fold_clear_sentinel(params.agent_id.clone());

    let channel_po = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        org_id,
        user_id,
        agent_id,
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
#[path = "create_message_channel_tests.rs"]
mod tests;
