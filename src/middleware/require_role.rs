//! 角色权限中间件
//!
//! 基于并查集角色继承体系，检查当前用户角色是否满足最低角色要求。
//! 用户角色在最低角色的祖先链上即可访问（上级角色满足下级要求）。

use crate::pkg::RequestContext;
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use common::api::ApiResponse;
use common::enums::UserRole;

/// 角色权限中间件
///
/// 检查当前用户角色是否满足 `min_role` 要求。
/// 用户角色在 min_role 的祖先链上（含自身）则通过，否则返回 403。
pub async fn require_role_middleware(min_role: UserRole, req: Request, next: Next) -> Response {
    let ctx = req.extensions().get::<RequestContext>().cloned();

    let user_role = ctx
        .as_ref()
        .and_then(|c| c.user_role())
        .map(UserRole::from_i32)
        .unwrap_or(UserRole::Member);

    if !UserRole::has_permission(user_role, min_role) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()>::error(403, "权限不足".to_string())),
        )
            .into_response();
    }

    next.run(req).await
}
