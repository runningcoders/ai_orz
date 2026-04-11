//! 更新用户信息接口

use common::api::{UpdateUserRequest, UpdateUserResponse};
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::{Extension, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use crate::service::domain::organization;
use std::time::{SystemTime, UNIX_EPOCH};

/// 获取当前时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 更新用户信息
pub async fn update_user(
    Extension(ctx): Extension<RequestContext>,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    
    let mut user = domain.user_manage().get_user_by_id(ctx.clone(), &user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", user_id)))?;
    
    // 更新字段
    if let Some(display_name) = req.display_name {
        user.display_name = Some(display_name);
    }
    if let Some(email) = req.email {
        user.email = Some(email);
    }
    if let Some(role) = req.role {
        // 从 i32 转换为 UserRole 枚举
        use common::enums::UserRole;
        let role_enum = match role {
            1 => UserRole::SuperAdmin,
            2 => UserRole::Admin,
            3 => UserRole::Member,
            _ => UserRole::Member,
        };
        user.role = role_enum;
    }
    if let Some(status) = req.status {
        // 从 i32 转换为 UserStatus 枚举
        use common::enums::UserStatus;
        user.status = UserStatus::from_i32(status);
    }
    if let Some(password_hash) = req.password_hash {
        user.password_hash = Some(password_hash);
    }
    user.updated_at = current_timestamp();
    
    domain.user_manage().update_user(ctx, &user).await?;
    
    let _role_name = user.user_role().map(|r: common::enums::UserRole| r.display_name().to_string()).unwrap_or_default();
    
    Ok((StatusCode::OK, Json(ApiResponse::success(UpdateUserResponse {
        user_id: user.id.clone().expect("id should not be None"),
        username: user.username.clone().expect("username should not be None"),
        display_name: if user.display_name.as_ref().map_or(true, |s| s.is_empty()) { None } else { user.display_name.clone() },
        email: if user.email.as_ref().map_or(true, |s| s.is_empty()) { None } else { user.email.clone() },
        role: user.user_role().map(|r| r as i32).unwrap_or(0),
        status: user.status.to_i32(),
    })).into_response()))
}
