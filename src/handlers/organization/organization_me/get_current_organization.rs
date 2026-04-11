//! 获取当前登录用户所在组织信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use common::api::{GetCurrentOrganizationResponse, OrganizationInfoResponse};
use crate::pkg::RequestContext;
use crate::{
    error::AppError,
    handlers::ApiResponse,
    service::domain::organization,
};

/// Get current authenticated user's organization information
/// 从 RequestContext 获取 organization_id，查询组织信息返回
pub async fn get_current_organization(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<GetCurrentOrganizationResponse>>), AppError> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx.organization_id.clone().ok_or_else(|| {
        AppError::BadRequest("未找到组织信息".to_string())
    })?;

    let domain = organization::domain();
    // 获取组织完整信息
    let org = domain.organization_manage().get_by_id(ctx, &org_id)
        .await?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

    // 转换为响应格式
    let info = OrganizationInfoResponse {
        organization_id: org.id.clone().expect("id should not be None"),
        name: org.name.clone().expect("name should not be None"),
        description: if org.description.as_ref().map_or(true, |s| s.is_empty()) { None } else { org.description.clone() },
        base_url: if org.base_url.as_ref().map_or(true, |s| s.is_empty()) { None } else { org.base_url.clone() },
        status: org.status.to_i32(),
        created_at: org.created_at,
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(GetCurrentOrganizationResponse {
            data: info,
        })),
    ))
}
