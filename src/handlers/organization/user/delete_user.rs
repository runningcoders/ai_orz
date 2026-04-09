//! 删除用户接口

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;

/// 删除用户请求
#[derive(Debug, Deserialize)]
pub struct DeleteUserRequest {
    /// 用户 ID
    pub user_id: String,
}

/// 删除用户响应
/// 空响应
#[derive(Debug, Serialize)]
pub struct DeleteUserResponse {
}

/// 删除用户
pub async fn delete_user(
    Extension(ctx): Extension<RequestContext>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.user_manage().delete_user(ctx, &user_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(DeleteUserResponse {})).into_response()))
}
