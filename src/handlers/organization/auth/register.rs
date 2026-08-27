//! 邀请码注册 handler（公开接口，不需要登录）

use axum::Json;
use axum::extract::{Extension, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use common::api::{
    ApiResponse, InviteCodeValidateRequest, InviteCodeValidateResponse, LoginResponse,
    RegisterByInviteRequest,
};
use common::error::Result;
use cookie::time;
use cookie::{Cookie, SameSite};

use crate::middleware::jwt_auth::JWT_COOKIE_NAME;
use crate::pkg::RequestContext;
use crate::pkg::jwt;
use crate::service::domain::organization;

/// 邀请码注册（公开）
/// POST /organization/auth/register
pub async fn register_by_invite(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<RegisterByInviteRequest>,
) -> Result<impl IntoResponse> {
    let domain = organization::domain();

    // 业务规则（邀请码校验、唯一性预检、成员创建）全部收敛在 Domain 层
    let user = domain.user_manage().register_member(ctx, req).await?;
    let organization_id = user.organization_id.clone();

    // 签发 JWT（注册后直接登录，返回 LoginResponse）
    let token = jwt::encode_jwt(
        user.id.as_str(),
        user.username.as_str(),
        &organization_id,
        Some(user.role.to_i32()),
    )?;

    let cookie = Cookie::build((JWT_COOKIE_NAME, token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(
            jwt::jwt_config().default_expiry_seconds(),
        ))
        .secure(false);

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        cookie.to_string().parse().unwrap(),
    );

    Ok((
        headers,
        (
            StatusCode::OK,
            Json(ApiResponse::success(LoginResponse {
                user_id: user.id.clone(),
                username: user.username.clone(),
                display_name: user.display_name.clone(),
                organization_id,
                token,
            })),
        ),
    ))
}

/// 校验邀请码有效性（前端注册表单输入时实时校验）
/// GET /organization/auth/invite/validate?invite_code=XXX
pub async fn validate_invite_code(
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<InviteCodeValidateRequest>,
) -> Result<impl IntoResponse> {
    let org = organization::domain()
        .organization_manage()
        .find_org_by_invite_code(ctx, &params.invite_code)
        .await?;

    let resp = match org {
        Some(o) => InviteCodeValidateResponse {
            valid: true,
            organization_id: Some(o.id),
            organization_name: Some(o.name),
        },
        None => InviteCodeValidateResponse {
            valid: false,
            organization_id: None,
            organization_name: None,
        },
    };

    Ok(Json(ApiResponse::success(resp)))
}
