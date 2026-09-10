//! Handler: GET /api/v1/user/me - Get current authenticated user information

use crate::middleware::jwt_auth::expired_jwt_cookie_header_value;
use crate::pkg::RequestContext;
use crate::service::domain::{finance, organization};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetCurrentUserRequest, GetCurrentUserResponse, UserInfoResponse};
use common::enums::UserRole;
use common::error::Result;
use common::models::StatsInterval;

/// Get current authenticated user information from request context
///
/// `with_model_call_stats=true` 时按需注入模型调用统计（打点 `user_id` 匹配口径）。
/// `/user/me` 是登录态回填的高频接口，不传该参数时零额外开销。
#[register_handler_tool(
    id = "get_current_user",
    name = "Get My Profile",
    description = "Get the profile of the currently authenticated user: user ID, username, display name, email, organization ID, role, status, and preferences. Optionally loads model-call stats (matched by the user_id tag in model-call events, hourly or daily within a time range). Returns the user info resolved from the session. Fails if the session has no user context or the account no longer exists.",
    params = "common::api::GetCurrentUserRequest"
)]
#[generate_http_handler]
pub async fn get_current_user(
    ctx: RequestContext,
    params: GetCurrentUserRequest,
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
        .get_user_by_id(ctx.clone(), &user_id)
        .await?
        // JWT 通过了但其引用的 user_id 在 DB 中已不存在（后端数据清空、
        // 用户被删除等），此时不是 404，而是「会话身份已失效」：返回 401
        // 并附 Set-Cookie 清掉 HttpOnly JWT，前端下一次请求立即出清登录态。
        .ok_or_else(|| {
            common::error::Error::unauthorized(format!(
                "当前登录身份已失效，请重新登录（用户 {user_id} 不存在）"
            ))
            .with_response_header(
                axum::http::header::SET_COOKIE.as_str(),
                expired_jwt_cookie_header_value(),
            )
        })?;

    // 按需注入模型调用统计（与 get_agent / get_task 的 fetch-options 协议对齐）
    let model_call_stats = if params.with_model_call_stats.unwrap_or(false) {
        let options = common::models::StatsFetchOptions {
            with_call_summary: true,
            with_token_summary: true,
            with_time_series: true,
            time_range: match (params.stats_time_start, params.stats_time_end) {
                (Some(start), Some(end)) => Some((start, end)),
                _ => None,
            },
            interval: params.stats_interval.as_deref().and_then(|s| {
                match s.to_lowercase().as_str() {
                    "hourly" => Some(StatsInterval::Hourly),
                    "daily" => Some(StatsInterval::Daily),
                    _ => None,
                }
            }),
        };
        Some(
            finance::domain()
                .model_provider_manage()
                .get_model_call_stats_for_user(ctx, &user_id, options)
                .await?,
        )
    } else {
        None
    };

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

    Ok(GetCurrentUserResponse {
        data: info,
        model_call_stats,
    })
}
