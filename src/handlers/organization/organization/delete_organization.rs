//! Handler: DELETE /api/v1/organizations/{id} - Delete an organization

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteOrganizationRequest, DeleteOrganizationResponse};
use common::error::Result;

/// Delete an organization (requires admin privileges)
/// 注意：此 handler 不注册为 Agent 工具（高危删除操作，仅管理员手动调用）。
#[generate_http_handler]
pub async fn delete_organization(
    ctx: RequestContext,
    params: DeleteOrganizationRequest,
) -> Result<DeleteOrganizationResponse> {
    let domain = organization::domain();
    domain
        .organization_manage()
        .delete(ctx, &params.organization_id)
        .await?;

    Ok(DeleteOrganizationResponse { success: true })
}
