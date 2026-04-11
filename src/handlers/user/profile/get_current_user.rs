//! 获取当前认证用户信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use common::api::{GetCurrentUserResponse, UserInfoResponse};
use common::enums::UserRole;
use crate::pkg::RequestContext;
use crate::{
    error::AppError,
    handlers::ApiResponse,
    service::domain::organization,
};

/// Get current authenticated user information
/// 从 RequestContext 中获取已认证用户信息并返回
pub async fn get_current_user(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<GetCurrentUserResponse>>), AppError> {
    // 从 RequestContext 获取当前用户 ID
    let user_id = ctx.user_id.clone().ok_or_else(|| {
        AppError::BadRequest("用户未登录".to_string())
    })?;

    // 通过 organization domain 获取用户完整信息
    let domain = organization::domain();
    let user = domain.user_manage().get_user_by_id(ctx, &user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    // 转换为响应格式
    let role = user.user_role();
    let role_name = match role {
        Some(UserRole::Member) => "成员",
        Some(UserRole::Admin) => "管理员",
        Some(UserRole::SuperAdmin) => "超级管理员",
        None => "未知",
    }.to_string();

    let info = UserInfoResponse {
        user_id: user.id.clone().expect("id should not be None"),
        username: user.username.clone().expect("username should not be None"),
        display_name: if user.display_name.as_ref().map_or(true, |s| s.is_empty()) { None } else { user.display_name.clone() },
        email: if user.email.as_ref().map_or(true, |s| s.is_empty()) { None } else { user.email.clone() },
        organization_id: user.organization_id.clone().expect("organization_id should not be None"),
        role: role.map(|r| r as i32).unwrap_or(0),
        role_name,
        status: user.status.to_i32(),
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(GetCurrentUserResponse {
            data: info,
        })),
    ))
}
