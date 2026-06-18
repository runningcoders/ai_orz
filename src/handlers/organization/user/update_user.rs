//! Handler: PUT /api/v1/organizations/users/{id} - Update user information

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateUserRequest, UpdateUserResponse};
use common::enums::{UserRole, UserStatus};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Update user information (requires admin permissions)
#[register_handler_tool(
    id = "update_user",
    name = "update_user",
    description = "Update user information including display name, email, role, status, and password hash. Requires admin permissions.",
    params = "common::api::UpdateUserRequest",
)]
#[generate_http_handler]
pub async fn update_user(
    ctx: RequestContext,
    params: UpdateUserRequest,
) -> Result<UpdateUserResponse, AppError> {
    let domain = organization::domain();

    let mut user = domain
        .user_manage()
        .get_user_by_id(ctx.clone(), &params.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", params.user_id)))?;

    // 更新字段
    if let Some(display_name) = params.display_name {
        user.display_name = display_name;
    }
    if let Some(email) = params.email {
        user.email = email;
    }
    if let Some(role) = params.role {
        // 从 i32 转换为 UserRole 枚举
        let role_enum = match role {
            0 => UserRole::SuperAdmin,
            1 => UserRole::Admin,
            2 => UserRole::Member,
            _ => UserRole::Member,
        };
        user.role = role_enum;
    }
    if let Some(status) = params.status {
        // 从 i32 转换为 UserStatus 枚举
        user.status = UserStatus::from_i32(status);
    }
    if let Some(password_hash) = params.password_hash {
        user.password_hash = password_hash;
    }
    user.updated_at = current_timestamp();

    domain.user_manage().update_user(ctx, &user).await?;

    let role_name = user.user_role().display_name().to_string();

    Ok(UpdateUserResponse {
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
        role: user.user_role() as i32,
        status: user.status.to_i32(),
    })
}