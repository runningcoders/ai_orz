//! Handler: PUT /api/v1/organizations/users/{id} - Update user information

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateUserRequest, UpdateUserResponse};
use common::enums::{UserRole, UserStatus};
use common::error::Result;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Update user information (requires admin permissions)
/// 注意：此 handler 不注册为 Agent 工具（涉及密码哈希等敏感字段，仅管理员手动调用）。
#[generate_http_handler]
pub async fn update_user(
    ctx: RequestContext,
    params: UpdateUserRequest,
) -> Result<UpdateUserResponse> {
    let domain = organization::domain();

    let mut user = domain
        .user_manage()
        .get_user_by_id(ctx.clone(), &params.user_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("User {} not found", params.user_id))
        })?;

    // 更新字段
    if let Some(display_name) = params.display_name {
        user.display_name = display_name;
    }
    if let Some(email) = params.email {
        user.email = email;
    }
    if let Some(role) = params.role {
        // 修复 E2E-4：手写映射改为统一入口 UserRole::from_i32（与 create_user 一致）
        user.role = UserRole::from_i32(role);
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

    let _role_name = user.user_role().display_name().to_string();

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
