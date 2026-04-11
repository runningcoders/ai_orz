//! 获取组织列表接口

use common::api::{ListOrganizationsResponse, OrganizationListItem};
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::service::domain::organization;
use crate::models::organization::OrganizationPo;

/// 获取组织列表
pub async fn list_organizations(
    Extension(ctx): Extension<RequestContext>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let orgs = domain.organization_manage().list_all(ctx.clone()).await?;
    let total = orgs.len() as u64;
    let items: Vec<OrganizationListItem> = orgs
        .into_iter()
        .map(|org: OrganizationPo| OrganizationListItem {
            organization_id: org.id.clone(),
            name: org.name.clone(),
            description: if org.description.is_empty() { None } else { Some(org.description.clone()) },
        })
        .collect();

    Ok((StatusCode::OK, Json(ApiResponse::success(ListOrganizationsResponse {
        data: items,
        total,
    }))).into_response())
}
