//! Handler: PUT /api/v1/organization/me - Update current authenticated user's organization information

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    OrganizationInfoResponse, UpdateCurrentOrganizationRequest, UpdateCurrentOrganizationResponse,
};
use common::constants::utils;

/// Update information for the currently authenticated user's organization (admin only)
#[register_handler_tool(
    id = "update_current_organization",
    name = "update_current_organization",
    description = "Update information for the organization that the currently authenticated user belongs to (requires admin privileges within the organization)",
    params = "common::api::UpdateCurrentOrganizationRequest"
)]
#[generate_http_handler]
pub async fn update_current_organization(
    ctx: RequestContext,
    params: UpdateCurrentOrganizationRequest,
) -> Result<UpdateCurrentOrganizationResponse> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("未找到组织信息".to_string()))?;

    let domain = organization::domain();
    // 获取当前组织信息
    let mut org = domain
        .organization_manage()
        .get_by_id(ctx.clone(), &org_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("组织不存在".to_string()))?;

    // 更新可修改字段
    if let Some(new_name) = params.name {
        org.name = new_name;
    }
    if let Some(new_description) = params.description {
        org.description = new_description;
    }
    if let Some(new_base_url) = params.base_url {
        org.base_url = new_base_url;
    }

    // 更新修改时间
    org.updated_at = utils::current_timestamp();
    if let Some(modifier_id) = ctx.user_id.clone() {
        org.modified_by = modifier_id;
    }

    // 保存更新
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

    Ok(UpdateCurrentOrganizationResponse { data })
}
