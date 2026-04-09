//! 删除用户接口

use common::api::DeleteUserResponse;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use common::constants::RequestContext;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::service::domain::organization;

/// 删除用户
pub async fn delete_user(
    Extension(ctx): Extension<RequestContext>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.user_manage().delete_user(ctx, &user_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(DeleteUserResponse {
        success: true,
    })).into_response()))
}
