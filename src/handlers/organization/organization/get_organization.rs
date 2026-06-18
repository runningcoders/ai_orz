//! Handler: GET /api/v1/organizations/{id} - Get organization basic information

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{GetOrganizationRequest, GetOrganizationResponse, OrganizationInfoResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// Get organization basic information by ID
#[register_handler_tool(
    id = "get_organization",
    name = "get_organization",
    description = "Get detailed information about a specific organization by its ID",
    params = "common::api::GetOrganizationRequest",
)]
#[generate_http_handler]
pub async fn get_organization(
    ctx: RequestContext,
    params: GetOrganizationRequest,
) -> Result<GetOrganizationResponse, AppError> {
    let domain = organization::domain();
    let org = domain.organization_manage().get_by_id(ctx, &params.organization_id).await?;

    let org = org.ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

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

    Ok(GetOrganizationResponse { data })
}