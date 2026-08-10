//! Handler: GET /api/v1/user/me - Get current authenticated user information

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetCurrentUserRequest, GetCurrentUserResponse, UserInfoResponse};
use common::enums::UserRole;
use common::error::Result;

/// Get current authenticated user information from request context
#[register_handler_tool(
    id = "get_current_user",
    name = "get_current_user",
    description = "Get information about the currently authenticated user",
    params = "common::api::GetCurrentUserRequest"
)]
#[generate_http_handler]
pub async fn get_current_user(
    ctx: RequestContext,
    _params: GetCurrentUserRequest,
) -> Result<GetCurrentUserResponse> {
    // 从 RequestContext 获取当前用户 ID
    let user_id = ctx
        .user_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("用户未登录".to_string()))?;

    // 通过 organization domain 获取用户完整信息
    let domain = organization::domain();
    let user = domain
        .user_manage()
        .get_user_by_id(ctx, &user_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("用户不存在".to_string()))?;

    // 转换为响应格式
    let role = user.user_role();
    let role_name = match role {
        UserRole::Member => "成员",
        UserRole::Admin => "管理员",
        UserRole::SuperAdmin => "超级管理员",
    }
    .to_string();

    let info = UserInfoResponse {
        user_id: user.id.clone(),
        username: user.username.clone(),
        display_name: if user.display_name.is_empty() {
            None
        } else {
            Some(user.display_name.clone())
        },
        email: if user.email.is_empty() {
            None
        } else {
            Some(user.email.clone())
        },
        organization_id: user.organization_id.clone(),
        role: role as i32,
        role_name,
        status: user.status.to_i32(),
        preferences: if user.preferences.is_empty() {
            None
        } else {
            Some(user.preferences.clone())
        },
    };

    Ok(GetCurrentUserResponse { data: info })
}
