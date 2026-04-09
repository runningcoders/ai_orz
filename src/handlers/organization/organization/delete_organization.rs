//! 删除组织接口

use common::api::DeleteOrganizationResponse;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use common::constants::RequestContext;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::service::domain::organization;

/// 删除组织
pub async fn delete_organization(
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.organization_manage().delete(ctx, &org_id)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(DeleteOrganizationResponse {
        success: true,
    })).into_response()))
}
