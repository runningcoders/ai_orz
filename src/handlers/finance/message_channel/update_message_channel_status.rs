//! 更新 Message Channel 状态

use axum::{
    extract::{Extension, Json, Path},
    Json as AxumJson,
};
use common::api::{
    ApiResponse, UpdateMessageChannelStatusRequest, UpdateMessageChannelStatusResponse,
};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 更新 Message Channel 状态
/// PUT /message-channels/{id}/status
pub async fn update_message_channel_status(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMessageChannelStatusRequest>,
) -> Result<AxumJson<ApiResponse<UpdateMessageChannelStatusResponse>>, AppError> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("当前请求缺少组织上下文".to_string()))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        return Err(AppError::NotFound(format!("MessageChannel {} not found", id)));
    }

    channel
        .transition_status(req.status, user_id)
        .map_err(AppError::BadRequest)?;

    domain()
        .message_channel_manage()
        .update_message_channel(ctx, &channel)
        .await?;

    Ok(AxumJson(ApiResponse::success(to_detail(&channel))))
}
