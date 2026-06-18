//! Handler: GET /api/v1/organizations - List all organizations in the system

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{ListOrganizationsRequest, ListOrganizationsResponse, OrganizationListItem};
use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// List all organizations available to the current user
#[register_handler_tool(
    id = "list_organizations",
    name = "list_organizations",
    description = "List all organizations in the system that the current user has access to",
    params = "common::api::ListOrganizationsRequest",
)]
#[generate_http_handler]
pub async fn list_organizations(
    ctx: RequestContext,
    _params: ListOrganizationsRequest,
) -> Result<ListOrganizationsResponse, AppError> {
    let domain = organization::domain();
    let orgs = domain.organization_manage().list_all(ctx).await?;
    let total = orgs.len() as u64;
    let items: Vec<OrganizationListItem> = orgs
        .into_iter()
        .map(|org: OrganizationPo| OrganizationListItem {
            organization_id: org.id.clone(),
            name: org.name.clone(),
            description: if org.description.is_empty() {
                None
            } else {
                Some(org.description.clone())
            },
        })
        .collect();

    Ok(ListOrganizationsResponse {
        data: items,
        total,
    })
}