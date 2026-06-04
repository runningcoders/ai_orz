//! 创建 Message Channel

use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use common::api::{ApiResponse, CreateMessageChannelRequest, CreateMessageChannelResponse};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 创建 Message Channel
/// POST /message-channels
pub async fn create_message_channel(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateMessageChannelRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateMessageChannelResponse>>), AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = req.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let channel_po = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        org_id,
        user_id,
        req.agent_id.clone(),
        req.channel_type,
        req.channel_name.clone(),
        req.webhook_url.clone(),
        req.access_token.clone(),
        req.secret.clone(),
        ChannelConfig {
            lark_app_id: req.lark_app_id.clone(),
            lark_app_secret: req.lark_app_secret.clone(),
            lark_encrypt_key: req.lark_encrypt_key.clone(),
            lark_verification_token: req.lark_verification_token.clone(),
            wechat_app_id: req.wechat_app_id.clone(),
            wechat_app_secret: req.wechat_app_secret.clone(),
            wechat_open_id: req.wechat_open_id.clone(),
            email_smtp_host: req.email_smtp_host.clone(),
            email_smtp_port: req.email_smtp_port,
            email_username: req.email_username.clone(),
            email_password: req.email_password.clone(),
            email_from_address: req.email_from_address.clone(),
            email_to_address: req.email_to_address.clone(),
            slack_bot_token: req.slack_bot_token.clone(),
            slack_channel_id: req.slack_channel_id.clone(),
            webhook_method: req.webhook_method.clone(),
            webhook_headers: None,
            webhook_body_template: req.webhook_body_template.clone(),
            extra: None,
        },
        ctx.uid(),
    );
    let channel = MessageChannel::from_po(channel_po);

    domain()
        .message_channel_manage()
        .create_message_channel(ctx, &channel)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(to_detail(&channel))),
    ))
}
