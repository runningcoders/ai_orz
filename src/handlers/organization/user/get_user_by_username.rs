//! 根据用户名查询用户接口（用于登录）

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
};
use common::api::ApiResponse;
use serde::Serialize;

/// 根据用户名查询用户响应
#[derive(Debug, Serialize)]
pub struct GetUserByUsernameResponse {
    /// 用户信息
    pub user: Option<UserPo>,
}

/// 根据用户名查询用户（用于登录）
pub async fn get_user_by_username(
    Extension(ctx): Extension<RequestContext>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let user = domain
        .user_manage()
        .find_by_username(ctx, &username)
        .await?;

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(GetUserByUsernameResponse { user })),
    )
        .into_response())
}
