//! 获取组织信息接口

use common::api::OrganizationInfoResponse;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::service::domain::organization;

/// 获取组织信息
pub async fn get_organization(
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let org = domain.organization_manage().get_by_id(ctx, &org_id).await?;

    match org {
        Some(org) => Ok((StatusCode::OK, Json(ApiResponse::success(OrganizationInfoResponse {
            organization_id: org.id.clone(),
            name: org.name.clone(),
            description: if org.description.is_empty() { None } else { Some(org.description.clone()) },
            base_url: if org.base_url.is_empty() { None } else { Some(org.base_url.clone()) },
            status: org.status.to_i32(),
            created_at: org.created_at,
        }))).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(ApiResponse::<OrganizationInfoResponse>::error(404, "组织不存在".to_string(), ))).into_response()),
    }
}
