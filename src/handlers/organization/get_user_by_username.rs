//! 根据用户名查询用户接口
//! 主要用于登录验证

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 根据用户名查询用户响应
#[derive(Debug, Serialize)]
pub struct GetUserByUsernameResponse {
    /// 用户信息
    pub user: Option<UserPo>,
}

/// 根据用户名查询用户
pub async fn get_user_by_username(
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain::domain();
    let user = domain.user_manage().find_by_username(ctx, &username)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(GetUserByUsernameResponse {
        user,
    })).into_response()))
}
