//! Handler: PUT /api/v1/user/me - Update current authenticated user information

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateCurrentUserRequest, UpdateCurrentUserResponse, UserInfoResponse};
use common::constants::utils;
use common::enums::UserRole;
use common::error::Result;

/// Update current user's own information (display name, email, password)
#[register_handler_tool(
    id = "update_current_user",
    name = "update_current_user",
    description = "Update information for the currently authenticated user, allows changing display name, email, and password",
    params = "common::api::UpdateCurrentUserRequest"
)]
#[generate_http_handler]
pub async fn update_current_user(
    ctx: RequestContext,
    params: UpdateCurrentUserRequest,
) -> Result<UpdateCurrentUserResponse> {
    // 从 RequestContext 获取当前用户 ID（JWT 已经验证过）
    let user_id = ctx
        .user_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("用户未登录".to_string()))?;

    // 通过 organization domain 获取用户当前信息
    let domain = organization::domain();
    let mut user = domain
        .user_manage()
        .get_user_by_id(ctx.clone(), &user_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("用户不存在".to_string()))?;

    // 权限检查：只能修改自己，JWT 已经认证过，这里用户ID匹配就是合法的
    // 不需要额外权限校验，JWT 中间件已经保证 user_id 是合法的当前用户

    // 更新可修改字段：只允许修改显示名称、邮箱、密码哈希
    // 用户不能修改自己的角色、状态、组织ID等敏感信息
    if let Some(new_display_name) = params.display_name {
        user.display_name = new_display_name;
    }
    if let Some(new_email) = params.email {
        user.email = new_email;
    }
    if let Some(new_password_hash) = params.password_hash {
        user.password_hash = new_password_hash;
    }

    // 更新修改时间和修改人
    user.updated_at = utils::current_timestamp();
    if let Some(modifier_id) = ctx.user_id.clone() {
        user.modified_by = modifier_id;
    }

    // 使用已有的 domain 方法更新用户信息，复用抽象层逻辑
    let domain = organization::domain();
    domain.user_manage().update_user(ctx, &user).await?;

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
    };

    Ok(UpdateCurrentUserResponse { data: info })
}
