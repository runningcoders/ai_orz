//! 获取组织信息接口

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
use crate::models::organization::OrganizationPo;

/// 获取组织请求
#[derive(Debug, Deserialize)]
pub struct GetOrganizationRequest {
    /// 组织 ID
    pub org_id: String,
}

/// 获取组织响应
#[derive(Debug, Serialize)]
pub struct GetOrganizationResponse {
    /// 组织信息
    pub organization: OrganizationPo,
}

/// 获取组织信息
pub async fn get_organization(
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain();
    let org = domain.organization_manage().get_by_id(ctx, &org_id)?;

    match org {
        Some(org) => Ok((StatusCode::OK, Json(ApiResponse::success(GetOrganizationResponse {
            organization: org,
        }))).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(ApiResponse::<GetOrganizationResponse>::error(404, "组织不存在".to_string(), ))).into_response()),
    }
}
