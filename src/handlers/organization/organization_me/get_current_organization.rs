//! Handler: GET /api/v1/organization/me - Get current authenticated user's organization information

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    GetCurrentOrganizationRequest, GetCurrentOrganizationResponse, OrganizationInfoResponse,
};

/// Get information for the currently authenticated user's organization
#[register_handler_tool(
    id = "get_current_organization",
    name = "get_current_organization",
    description = "Get detailed information about the organization that the currently authenticated user belongs to",
    params = "common::api::GetCurrentOrganizationRequest"
)]
#[generate_http_handler]
pub async fn get_current_organization(
    ctx: RequestContext,
    _params: GetCurrentOrganizationRequest,
) -> Result<GetCurrentOrganizationResponse, AppError> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("未找到组织信息".to_string()))?;

    let domain = organization::domain();
    // 获取组织完整信息
    let org = domain
        .organization_manage()
        .get_by_id(ctx, &org_id)
        .await?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

    // 转换为响应格式
    let data = OrganizationInfoResponse {
        organization_id: org.id.clone(),
        name: org.name.clone(),
        description: if org.description.is_empty() {
            None
        } else {
            Some(org.description.clone())
        },
        base_url: if org.base_url.is_empty() {
            None
        } else {
            Some(org.base_url.clone())
        },
        status: org.status.to_i32(),
        created_at: org.created_at,
    };

    Ok(GetCurrentOrganizationResponse { data })
}
