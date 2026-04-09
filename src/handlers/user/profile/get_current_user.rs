//! 获取当前认证用户信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    models::user::UserPo,
    pkg::constants::{request_context::RequestContext, UserRole},
    service::dao,
};
use serde::{Deserialize, Serialize};

/// 当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色编号
    pub role: i32,
    /// 角色名称
    pub role_name: String,
    /// 用户状态
    pub status: i32,
}

/// 获取当前用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentUserResponse {
    /// 用户信息数据
    pub data: UserInfoResponse,
}

/// Get current authenticated user information
/// 从 RequestContext 中获取已认证用户信息并返回
pub async fn get_current_user(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<GetCurrentUserResponse>>), AppError> {
    // 从 RequestContext 获取当前用户 ID
    let user_id = ctx.user_id.clone().ok_or_else(|| {
        AppError::BadRequest("用户未登录".to_string())
    })?;

    // 直接调用 DAO 获取用户完整信息
    let user = dao::user::dao().find_by_id(ctx, &user_id)?
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
        user_id: user.id.clone(),
        username: user.username.clone(),
        display_name: if user.display_name.is_empty() { None } else { Some(user.display_name.clone()) },
        email: if user.email.is_empty() { None } else { Some(user.email.clone()) },
        organization_id: user.organization_id.clone(),
        role: role.map(|r| r as i32).unwrap_or(0),
        role_name,
        status: user.status,
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(GetCurrentUserResponse {
            data: info,
        })),
    ))
}
