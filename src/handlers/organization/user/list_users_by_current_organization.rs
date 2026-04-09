//! 获取当前用户所在组织下的所有用户列表接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use common::api::{ListUsersResponse, UserListItem};
use common::constants::RequestContext;
use crate::{
    error::AppError,
    handlers::ApiResponse,
    service::domain::organization,
};

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
    let total = users.len() as u64;

    // 转换为响应格式
    let data = users.into_iter().map(|user| UserListItem {
        user_id: user.id.clone(),
        username: user.username.clone(),
        display_name: if user.display_name.is_empty() { None } else { Some(user.display_name.clone()) },
        email: if user.email.is_empty() { None } else { Some(user.email.clone()) },
        role: user.user_role().map(|r| r as i32).unwrap_or(0),
        role_name: user.user_role().map(|r| r.display_name().to_string()).unwrap_or_default(),
        status: user.status,
        created_at: user.created_at,
    }).collect();

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(ListUsersResponse {
            data,
            total,
        })),
    ))
}
