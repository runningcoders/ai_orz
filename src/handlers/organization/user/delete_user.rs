//! 删除用户接口

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
};
use common::api::ApiResponse;
use common::api::DeleteUserResponse;

/// 删除用户
pub async fn delete_user(
    Extension(ctx): Extension<RequestContext>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.user_manage().delete_user(ctx, &user_id).await?;

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(DeleteUserResponse { success: true })).into_response(),
    ))
}
