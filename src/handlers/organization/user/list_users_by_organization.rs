//! Handler: GET /api/v1/organizations/{id}/users - List all users by organization ID

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListUsersByOrganizationRequest, ListUsersResponse, UserListItem};

/// List all users belonging to a specific organization by organization ID
#[register_handler_tool(
    id = "list_users_by_organization",
    name = "list_users_by_organization",
    description = "List all users in a specified organization",
    params = "common::api::ListUsersByOrganizationRequest"
)]
#[generate_http_handler]
pub async fn list_users_by_organization(
    ctx: RequestContext,
    params: ListUsersByOrganizationRequest,
) -> Result<ListUsersResponse> {
    let domain = organization::domain();
    let users = domain
        .user_manage()
        .find_by_organization_id(ctx, &params.organization_id)
        .await?;
    let total = users.len() as u64;

    // 转换为响应格式
    let data = users
        .into_iter()
        .map(|user| UserListItem {
            user_id: user.id.clone(),
            username: user.username.clone(),
            display_name: if user.display_name.is_empty() {
                None
            } else {
                Some(user.display_name.clone())
            },
            email: if user.email.is_empty() {
                None
            } else {
                Some(user.email.clone())
            },
            role: user.user_role() as i32,
            role_name: user.user_role().display_name().to_string(),
            status: user.status.to_i32(),
            created_at: user.created_at,
        })
        .collect();

    Ok(ListUsersResponse { data, total })
}
