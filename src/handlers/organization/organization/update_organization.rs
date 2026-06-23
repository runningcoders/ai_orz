//! Handler: PUT /api/v1/organizations/{id} - Update organization information (admin)

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    GetOrganizationResponse, OrganizationInfoResponse, UpdateOrganizationRequest,
    UpdateOrganizationResponse,
};
use common::constants::utils;

/// Update organization information (admin only)
#[register_handler_tool(
    id = "update_organization",
    name = "update_organization",
    description = "Update organization information including name, description, base URL, and status (requires admin privileges)",
    params = "common::api::UpdateOrganizationRequest"
)]
#[generate_http_handler]
pub async fn update_organization(
    ctx: RequestContext,
    params: UpdateOrganizationRequest,
) -> Result<UpdateOrganizationResponse, AppError> {
    let domain = organization::domain();

    let mut org = domain
        .organization_manage()
        .get_by_id(ctx.clone(), &params.organization_id)
        .await?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

    // 更新字段
    if let Some(name) = params.name {
        org.name = name;
    }
    if let Some(description) = params.description {
        org.description = description;
    }
    if let Some(base_url) = params.base_url {
        org.base_url = base_url;
    }
    if let Some(status) = params.status {
        org.status = status.into();
    }
    org.updated_at = utils::current_timestamp();

    domain.organization_manage().update(ctx, &org).await?;

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

    Ok(UpdateOrganizationResponse { data })
}
