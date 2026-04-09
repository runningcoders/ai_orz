//! 更新用户信息接口

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 更新用户请求
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    /// 用户信息
    pub user: UserPo,
}

/// 更新用户响应
/// 空响应
#[derive(Debug, Serialize)]
pub struct UpdateUserResponse {
}

/// 更新用户信息
pub async fn update_user(
    Extension(ctx): Extension<RequestContext>,
    req: Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.user_manage().update_user(ctx, &req.user)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(UpdateUserResponse {})).into_response()))
}
