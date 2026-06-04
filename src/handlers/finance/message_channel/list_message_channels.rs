//! 列出 Message Channel

use axum::{
    extract::{Extension, Query},
    Json,
};
use common::api::{ApiResponse, MessageChannelListItem, MessageChannelListQuery};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dao::message_channel::MessageChannelQuery;
use crate::service::domain::finance::domain;

use super::response::to_list_item;

/// 列出 Message Channel
/// GET /message-channels
pub async fn list_message_channels(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<MessageChannelListQuery>,
) -> Result<Json<ApiResponse<Vec<MessageChannelListItem>>>, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = req.user_id.clone().unwrap_or_else(|| ctx.uid());
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let channels = domain()
        .message_channel_manage()
        .query_channels(
            ctx,
            MessageChannelQuery {
                org_id: Some(org_id),
                user_id: Some(user_id),
                agent_id: req.agent_id.clone(),
                channel_type: req.channel_type,
                only_enabled: req.only_enabled.unwrap_or(false),
                limit: req.limit,
                offset: req.offset,
                ..Default::default()
            },
        )
        .await?;

    let responses = channels.iter().map(to_list_item).collect();
    Ok(Json(ApiResponse::success(responses)))
}
