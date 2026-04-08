//! 删除组织接口

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

/// 删除组织请求
#[derive(Debug, Deserialize)]
pub struct DeleteOrganizationRequest {
    /// 组织 ID
    pub org_id: String,
}

/// 删除组织响应
/// 空响应
#[derive(Debug, Serialize)]
pub struct DeleteOrganizationResponse {
}

/// 删除组织
pub async fn delete_organization(
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain();
    domain.organization_manage().delete(ctx, &org_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(DeleteOrganizationResponse {})).into_response()))
}
