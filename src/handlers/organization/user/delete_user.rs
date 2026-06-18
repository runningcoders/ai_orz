//! Handler: DELETE /api/v1/organizations/users/{id} - Delete a user

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{DeleteUserRequest, DeleteUserResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// Delete an existing user by ID (requires admin permissions)
#[register_handler_tool(
    id = "delete_user",
    name = "delete_user",
    description = "Delete a user from the organization. Requires admin permissions.",
    params = "common::api::DeleteUserRequest",
)]
#[generate_http_handler]
pub async fn delete_user(
    ctx: RequestContext,
    params: DeleteUserRequest,
) -> Result<DeleteUserResponse, AppError> {
    let domain = organization::domain();
    domain.user_manage().delete_user(ctx, &params.user_id).await?;

    Ok(DeleteUserResponse { success: true })
}