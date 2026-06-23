//! Handler: DELETE /api/v1/organizations/{id} - Delete an organization

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteOrganizationRequest, DeleteOrganizationResponse};

/// Delete an organization (requires admin privileges)
#[register_handler_tool(
    id = "delete_organization",
    name = "delete_organization",
    description = "Delete an existing organization, requires admin privileges",
    params = "common::api::DeleteOrganizationRequest"
)]
#[generate_http_handler]
pub async fn delete_organization(
    ctx: RequestContext,
    params: DeleteOrganizationRequest,
) -> Result<DeleteOrganizationResponse, AppError> {
    let domain = organization::domain();
    domain
        .organization_manage()
        .delete(ctx, &params.organization_id)
        .await?;

    Ok(DeleteOrganizationResponse { success: true })
}
