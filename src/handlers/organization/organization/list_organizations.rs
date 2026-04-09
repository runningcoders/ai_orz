//! 获取组织列表接口

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use crate::service::domain::organization;
use crate::models::organization::OrganizationPo;

/// 获取组织列表响应
#[derive(Debug, Serialize)]
pub struct ListOrganizationsResponse {
    /// 组织列表
    pub organizations: Vec<OrganizationPo>,
}

/// 获取组织列表
pub async fn list_organizations(
    Extension(ctx): Extension<RequestContext>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let orgs = domain.organization_manage().list_all(ctx)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(ListOrganizationsResponse {
        organizations: orgs,
    }))).into_response())
}
