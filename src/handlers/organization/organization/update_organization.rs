//! 更新组织信息接口

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use axum::{
    extract::{Json},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;
use crate::models::organization::OrganizationPo;

/// 更新组织请求
#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationRequest {
    /// 组织信息
    pub organization: OrganizationPo,
}

/// 更新组织响应
/// 空响应
#[derive(Debug, Serialize)]
pub struct UpdateOrganizationResponse {
}

/// 更新组织信息
pub async fn update_organization(
    headers: HeaderMap,
    req: Json<UpdateOrganizationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = extract_ctx(&headers);
    let domain = organization::domain();
    domain.organization_manage().update(ctx, &req.organization)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(UpdateOrganizationResponse {})).into_response()))
}
