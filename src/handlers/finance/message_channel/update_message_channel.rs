//! 更新 Message Channel

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateMessageChannelRequest, UpdateMessageChannelResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 更新 Message Channel
/// PUT /message-channels/{id}
pub async fn update_message_channel(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMessageChannelRequest>,
) -> Result<AxumJson<ApiResponse<UpdateMessageChannelResponse>>, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != current_user_id {
        return Err(AppError::NotFound(format!(
            "MessageChannel {} not found",
            id
        )));
    }

    if let Some(user_id) = req.user_id {
        channel.po.user_id = user_id;
    }
    if let Some(agent_id) = req.agent_id {
        channel.po.agent_id = Some(agent_id);
    }
    if let Some(channel_type) = req.channel_type {
        channel.po.channel_type = channel_type;
    }
    if let Some(channel_name) = req.channel_name {
        channel.po.channel_name = channel_name;
    }
    if let Some(webhook_url) = req.webhook_url {
        channel.po.webhook_url = Some(webhook_url);
    }
    if let Some(access_token) = req.access_token {
        channel.po.access_token = Some(access_token);
    }
    if let Some(secret) = req.secret {
        channel.po.secret = Some(secret);
    }

    let config = &mut channel.po.config_json.0;
    if let Some(value) = req.lark_app_id {
        config.lark_app_id = Some(value);
    }
    if let Some(value) = req.lark_app_secret {
        config.lark_app_secret = Some(value);
    }
    if let Some(value) = req.lark_encrypt_key {
        config.lark_encrypt_key = Some(value);
    }
    if let Some(value) = req.lark_verification_token {
        config.lark_verification_token = Some(value);
    }
    if let Some(value) = req.wechat_app_id {
        config.wechat_app_id = Some(value);
    }
    if let Some(value) = req.wechat_app_secret {
        config.wechat_app_secret = Some(value);
    }
    if let Some(value) = req.wechat_open_id {
        config.wechat_open_id = Some(value);
    }
    if let Some(value) = req.email_smtp_host {
        config.email_smtp_host = Some(value);
    }
    if let Some(value) = req.email_smtp_port {
        config.email_smtp_port = Some(value);
    }
    if let Some(value) = req.email_username {
        config.email_username = Some(value);
    }
    if let Some(value) = req.email_password {
        config.email_password = Some(value);
    }
    if let Some(value) = req.email_from_address {
        config.email_from_address = Some(value);
    }
    if let Some(value) = req.email_to_address {
        config.email_to_address = Some(value);
    }
    if let Some(value) = req.slack_bot_token {
        config.slack_bot_token = Some(value);
    }
    if let Some(value) = req.slack_channel_id {
        config.slack_channel_id = Some(value);
    }
    if let Some(value) = req.webhook_method {
        config.webhook_method = Some(value);
    }
    if let Some(value) = req.webhook_body_template {
        config.webhook_body_template = Some(value);
    }

    channel.po.modified_by = current_user_id;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(AxumJson(ApiResponse::success(to_detail(&channel))))
}
