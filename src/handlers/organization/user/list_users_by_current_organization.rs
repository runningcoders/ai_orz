//! 获取当前用户所在组织下的所有用户列表接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    pkg::constants::request_context::RequestContext,
    service::domain::organization,
};
use serde::{Deserialize, Serialize};

/// 用户列表项响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListItemResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 角色编号
    pub role: i32,
    /// 用户状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// 获取当前组织用户列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUsersResponse {
    /// 用户列表
    pub data: Vec<UserListItemResponse>,
}

/// 获取当前用户所在组织下的所有用户列表
/// 从 RequestContext 提取 organization_id，直接返回列表，不需要前端传参
pub async fn list_users_by_current_organization(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<ListUsersResponse>>), AppError> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx.organization_id.clone().ok_or_else(|| {
        AppError::BadRequest("未找到组织信息".to_string())
    })?;

    let domain = organization::domain();
    // 获取组织下所有用户
    let users = domain.user_manage().find_by_organization_id(ctx, &org_id)?;

    // 转换为响应格式
    let data = users.into_iter().map(|user| UserListItemResponse {
        user_id: user.id.clone(),
        username: user.username.clone(),
        display_name: if user.display_name.is_empty() { None } else { Some(user.display_name.clone()) },
        email: if user.email.is_empty() { None } else { Some(user.email.clone()) },
        role: user.user_role().map(|r| r as i32).unwrap_or(0),
        status: user.status,
        created_at: user.created_at,
    }).collect();

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(ListUsersResponse {
            data,
        })),
    ))
}
