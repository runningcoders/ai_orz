//! Handler: GET /api/v1/organizations/users - List all users in current organization

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListUsersByCurrentOrganizationRequest, ListUsersResponse, UserListItem};
use common::bail_err;

/// List all users belonging to the current authenticated user's organization
#[register_handler_tool(
    id = "list_users_by_current_organization",
    name = "list_users_by_current_organization",
    description = "List all users in the organization that the current authenticated user belongs to",
    params = "common::api::ListUsersByCurrentOrganizationRequest"
)]
#[generate_http_handler]
pub async fn list_users_by_current_organization(
    ctx: RequestContext,
    _params: ListUsersByCurrentOrganizationRequest,
) -> Result<ListUsersResponse> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("未找到组织信息".to_string()))?;

    let domain = organization::domain();
    // 获取组织下所有用户
    let users = domain
        .user_manage()
        .find_by_organization_id(ctx, &org_id)
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
