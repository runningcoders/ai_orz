//! 根据组织 ID 查询用户列表接口

use crate::error::AppError;
use crate::handlers::ApiResponse;
use common::constants::RequestContext;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 根据组织 ID 查询用户列表响应
#[derive(Debug, Serialize)]
pub struct ListUsersByOrganizationResponse {
    /// 用户列表
    pub users: Vec<UserPo>,
}

/// 根据组织 ID 查询用户列表
pub async fn list_users_by_organization(
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let users = domain.user_manage().find_by_organization_id(ctx, &org_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(ListUsersByOrganizationResponse {
        users,
    }))).into_response())
}
