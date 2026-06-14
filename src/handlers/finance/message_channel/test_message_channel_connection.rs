//! 测试 Message Channel 连通性

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, TestMessageChannelConnectionResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 测试 Message Channel 连通性
/// POST /message-channels/{id}/test
pub async fn test_message_channel_connection(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TestMessageChannelConnectionResponse>>, AppError> {
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
        .get_message_channel(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("MessageChannel {} not found", id)))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        return Err(AppError::NotFound(format!(
            "MessageChannel {} not found",
            id
        )));
    }

    let response = match domain()
        .message_channel_manage()
        .test_message_channel(ctx, &channel)
        .await
    {
        Ok(()) => TestMessageChannelConnectionResponse {
            success: true,
            error: None,
        },
        Err(e) => TestMessageChannelConnectionResponse {
            success: false,
            error: Some(e.to_string()),
        },
    };

    Ok(Json(ApiResponse::success(response)))
}
