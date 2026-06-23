//! Handler: GET /api/v1/message-channels/{id} - Get message channel detailed information

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetMessageChannelRequest, GetMessageChannelResponse, MessageChannelDetail};

use super::response::to_detail;

/// Get detailed information about a specific message channel
#[register_handler_tool(
    id = "get_message_channel",
    name = "get_message_channel",
    description = "Get detailed information about a specific message channel",
    params = "common::api::GetMessageChannelRequest"
)]
#[generate_http_handler]
pub async fn get_message_channel(
    ctx: RequestContext,
    params: GetMessageChannelRequest,
) -> Result<GetMessageChannelResponse, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", params.id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        return Err(AppError::NotFound(format!(
            "MessageChannel {} not found",
            params.id
        )));
    }

    Ok(to_detail(&channel))
}
