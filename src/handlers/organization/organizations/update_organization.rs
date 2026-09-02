//! Handler: PUT /api/v1/organizations/{id} - Update organization information (admin)

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    OrganizationInfoResponse, UpdateOrganizationRequest, UpdateOrganizationResponse,
};
use common::constants::utils;
use common::enums::UserRole;
use common::error::{Error, Result};

/// Update organization information (admin only)
#[register_handler_tool(
    id = "update_organization",
    name = "Update Organization",
    description = "Update an organization by ID: name, description, base URL, and status; org-level config changes additionally require the SuperAdmin role. Returns the updated organization info including config. Use update_current_organization to modify the caller's own organization without specifying an ID.",
    params = "common::api::UpdateOrganizationRequest"
)]
#[generate_http_handler]
pub async fn update_organization(
    ctx: RequestContext,
    params: UpdateOrganizationRequest,
) -> Result<UpdateOrganizationResponse> {
    let domain = organization::domain();

    let mut org = domain
        .organization_manage()
        .get_by_id(ctx.clone(), &params.organization_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("组织不存在".to_string()))?;

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
    org.updated_at = utils::current_timestamp_ms();

    // 组织级配置：仅超级管理员可修改
    if let Some(cfg) = params.config {
        let role = ctx
            .user_role()
            .map(UserRole::from_i32)
            .unwrap_or(UserRole::Member);
        if !UserRole::has_permission(role, UserRole::SuperAdmin) {
            return Err(Error::forbidden("权限不足，仅超级管理员可修改组织级配置"));
        }
        domain
            .organization_manage()
            .update_org_config(ctx.clone(), &params.organization_id, &cfg)
            .await?;
    }

    domain
        .organization_manage()
        .update(ctx.clone(), &org)
        .await?;

    // 读取组织级配置（可能刚被更新），随响应一并返回
    let config = domain
        .organization_manage()
        .get_org_config(ctx, &org.id)
        .await?;

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
        config,
    };

    Ok(UpdateOrganizationResponse { data })
}
