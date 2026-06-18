//! Handler: GET /api/v1/organizations/users/by-username/{username} - Get user by username

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{GetUserByUsernameRequest, GetUserByUsernameResponse};
use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// Find a user by username (used for login authentication)
#[register_handler_tool(
    id = "get_user_by_username",
    name = "get_user_by_username",
    description = "Find a user by username, used for authentication",
    params = "common::api::GetUserByUsernameRequest",
)]
#[generate_http_handler]
pub async fn get_user_by_username(
    ctx: RequestContext,
    params: GetUserByUsernameRequest,
) -> Result<GetUserByUsernameResponse, AppError> {
    let domain = organization::domain();
    let user = domain
        .user_manage()
        .find_by_username(ctx, &params.username)
        .await?;

    let user_response = user.map(|u| {
        let role = u.user_role();
        let role_name = match role {
            common::enums::UserRole::Member => "成员",
            common::enums::UserRole::Admin => "管理员",
            common::enums::UserRole::SuperAdmin => "超级管理员",
        }
        .to_string();

        common::api::UserInfoResponse {
            user_id: u.id.clone(),
            username: u.username.clone(),
            display_name: if u.display_name.is_empty() {
                None
            } else {
                Some(u.display_name.clone())
            },
            email: if u.email.is_empty() {
                None
            } else {
                Some(u.email.clone())
            },
            organization_id: u.organization_id.clone(),
            role: role as i32,
            role_name,
            status: u.status.to_i32(),
        }
    });

    Ok(GetUserByUsernameResponse { user: user_response })
}