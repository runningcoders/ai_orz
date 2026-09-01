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
    name = "Update My Profile",
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

    // 更新可修改字段：只允许修改显示名称、邮箱、密码哈希、偏好自述
    // 用户不能修改自己的角色、状态、组织ID等敏感信息
    if let Some(new_display_name) = params.display_name {
        user.display_name = new_display_name;
    }
    if let Some(new_email) = params.email {
        user.email = new_email;
    }
    if let Some(new_password) = params.password {
        user.password_hash = crate::pkg::password::hash_password(&new_password)?;
    }
    // 偏好自述：仅限真人 HTTP 会话修改。本 handler 同时注册为 Agent 工具，
    // Agent 上下文（ctx.agent_id 有值）调用时忽略该字段，维持「Agent 不写用户表」边界
    if ctx.agent_id().is_none()
        && let Some(new_preferences) = params.preferences
    {
        user.preferences = new_preferences;
    }

    // 更新修改时间和修改人
    user.updated_at = utils::current_timestamp_ms();
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
        preferences: if user.preferences.is_empty() {
            None
        } else {
            Some(user.preferences.clone())
        },
    };

    Ok(UpdateCurrentUserResponse { data: info })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::UserPo;
    use common::enums::UserRole;
    use sqlx::SqlitePool;

    /// 初始化全层单例 + 插入测试用户，返回（真人会话 ctx，user_id）
    async fn setup(pool: SqlitePool) -> (RequestContext, String) {
        crate::service::init();
        let user_id = uuid::Uuid::now_v7().to_string();
        let user = UserPo::new(
            user_id.clone(),
            "org-test".to_string(),
            format!("prefuser_{}", uuid::Uuid::now_v7()),
            "偏好测试用户".to_string(),
            "pref-test@example.com".to_string(),
            "hash".to_string(),
            UserRole::Member,
            "system".to_string(),
        );
        let ctx = crate::pkg::request_context_test_support::new_test_ctx(&user_id, pool.clone());
        crate::service::dao::user::dao()
            .insert(ctx.clone(), &user)
            .await
            .unwrap();
        (ctx, user_id)
    }

    /// 真人会话：偏好自述可正常更新并回读
    #[sqlx::test]
    async fn test_update_preferences_success(pool: SqlitePool) {
        let (ctx, user_id) = setup(pool).await;

        let resp = update_current_user(
            ctx.clone(),
            UpdateCurrentUserRequest {
                display_name: None,
                email: None,
                password: None,
                preferences: Some("- 回复请用中文\n- 汇报要简洁".to_string()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            resp.data.preferences.as_deref(),
            Some("- 回复请用中文\n- 汇报要简洁")
        );

        // DB 回读一致
        let user = crate::service::dao::user::dao()
            .find_by_id(ctx, &user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.preferences, "- 回复请用中文\n- 汇报要简洁");
    }

    /// Agent 上下文（ctx.agent_id 有值）：preferences 字段被忽略，维持「Agent 不写用户表」边界
    #[sqlx::test]
    async fn test_update_preferences_ignored_in_agent_context(pool: SqlitePool) {
        let (ctx, user_id) = setup(pool).await;
        let mut agent_ctx = ctx.clone();
        agent_ctx.agent_id = Some("agent-test-1".to_string());

        let resp = update_current_user(
            agent_ctx,
            UpdateCurrentUserRequest {
                display_name: None,
                email: None,
                password: None,
                preferences: Some("Agent 试图写入的偏好".to_string()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            resp.data.preferences, None,
            "Agent 上下文不应能写入偏好字段"
        );

        // DB 中偏好仍为空
        let user = crate::service::dao::user::dao()
            .find_by_id(ctx, &user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(user.preferences.is_empty());
    }
}
