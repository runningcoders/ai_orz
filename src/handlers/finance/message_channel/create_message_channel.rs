//! Handler: POST /api/v1/message-channels - Create a new message channel for notifications

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{CreateMessageChannelRequest, CreateMessageChannelResponse, MessageChannelDetail};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// Create a new message channel for sending notifications to external services/users
#[register_handler_tool(
    id = "create_message_channel",
    name = "create_message_channel",
    description = "Create a new message channel for sending notifications to external services/users",
    params = "common::api::CreateMessageChannelRequest",
)]
#[generate_http_handler]
pub async fn create_message_channel(
    ctx: RequestContext,
    params: CreateMessageChannelRequest,
) -> Result<CreateMessageChannelResponse, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = params.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

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
            lark_app_id: params.lark_app_id.clone(),
            lark_app_secret: params.lark_app_secret.clone(),
            lark_encrypt_key: params.lark_encrypt_key.clone(),
            lark_verification_token: params.lark_verification_token.clone(),
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
            extra: None,
        },
        ctx.uid(),
    );
    let channel = MessageChannel::from_po(channel_po);

    domain()
        .message_channel_manage()
        .create_message_channel(ctx.clone(), &channel)
        .await?;

    Ok(to_detail(&channel))
}