//! 更新当前认证用户信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use common::api::{EmptyResponse, UpdateCurrentUserRequest};
use common::constants::{RequestContext, utils};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    service::dao,
    service::domain::organization,
};

/// Update current authenticated user information
/// 允许用户更新自己的可修改信息：显示名称、邮箱、密码
/// 通过组合已有的抽象方法实现：find_by_id (dao) + update_user (domain)
pub async fn update_current_user(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<UpdateCurrentUserRequest>,
) -> Result<(StatusCode, Json<ApiResponse<EmptyResponse>>), AppError> {
    // 从 RequestContext 获取当前用户 ID（JWT 已经验证过）
    let user_id = ctx.user_id.clone().ok_or_else(|| {
        AppError::BadRequest("用户未登录".to_string())
    })?;

    // 使用 DAO 获取用户当前信息
    let mut user = dao::user::dao().find_by_id(ctx.clone(), &user_id)?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    // 权限检查：只能修改自己，JWT 已经认证过，这里用户ID匹配就是合法的
    // 不需要额外权限校验，JWT 中间件已经保证 user_id 是合法的当前用户

    // 更新可修改字段：只允许修改显示名称、邮箱、密码哈希
    // 用户不能修改自己的角色、状态、组织ID等敏感信息
    if let Some(new_display_name) = req.display_name {
        user.display_name = new_display_name;
    }
    if let Some(new_email) = req.email {
        user.email = new_email;
    }
    if let Some(new_password_hash) = req.password_hash {
        user.password_hash = new_password_hash;
    }

    // 更新修改时间和修改人
    user.updated_at = utils::current_timestamp();
    if let Some(modifier_id) = ctx.user_id.clone() {
        user.modified_by = modifier_id;
    }

    // 使用已有的 domain 方法更新用户信息，复用抽象层逻辑
    let domain = organization::domain();
    domain.user_manage().update_user(ctx, &user)?;

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(EmptyResponse {})),
    ))
}
