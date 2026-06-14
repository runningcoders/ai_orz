//! 删除组织接口

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
};
use common::api::ApiResponse;
use common::api::DeleteOrganizationResponse;

/// 删除组织
pub async fn delete_organization(
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    domain.organization_manage().delete(ctx, &org_id).await?;

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(DeleteOrganizationResponse {
            success: true,
        }))
        .into_response(),
    ))
}
