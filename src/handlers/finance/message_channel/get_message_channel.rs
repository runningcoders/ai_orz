//! 获取单个 Message Channel

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetMessageChannelResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 获取 Message Channel
/// GET /message-channels/{id}
pub async fn get_message_channel(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetMessageChannelResponse>>, AppError> {
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
        .get_message_channel(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        return Err(AppError::NotFound(format!(
            "MessageChannel {} not found",
            id
        )));
    }

    Ok(Json(ApiResponse::success(to_detail(&channel))))
}
