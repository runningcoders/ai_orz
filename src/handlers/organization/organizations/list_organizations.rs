//! Handler: GET /api/v1/organizations - List all organizations in the system

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListOrganizationsRequest, ListOrganizationsResponse, OrganizationListItem};
use common::error::Result;

/// List all organizations available to the current user
#[register_handler_tool(
    id = "list_organizations",
    name = "List All Organizations",
    description = "List all non-deleted organizations in the system, newest first; each item carries ID, name, description, and scope. Returns the list plus a total count; no filters or pagination. Use get_organization or get_current_organization for a single organization's full details.",
    params = "common::api::ListOrganizationsRequest"
)]
#[generate_http_handler]
pub async fn list_organizations(
    ctx: RequestContext,
    _params: ListOrganizationsRequest,
) -> Result<ListOrganizationsResponse> {
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
            scope: org.scope.to_i32(),
        })
        .collect();

    Ok(ListOrganizationsResponse { data: items, total })
}
