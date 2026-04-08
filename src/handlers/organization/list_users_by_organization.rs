//! 根据组织 ID 查询所有用户接口

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;
use crate::models::user::UserPo;

/// 根据组织 ID 查询所有用户响应
#[derive(Debug, Serialize)]
pub struct ListUsersByOrganizationResponse {
    /// 用户列表
    pub users: Vec<UserPo>,
}

/// 根据组织 ID 查询所有用户
pub async fn list_users_by_organization(
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain::domain();
    let users = domain.user_manage().find_by_organization_id(ctx, &org_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(ListUsersByOrganizationResponse {
        users,
    })).into_response()))
}
