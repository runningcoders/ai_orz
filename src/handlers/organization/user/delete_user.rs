//! Handler: DELETE /api/v1/organizations/users/{id} - Delete a user

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteUserRequest, DeleteUserResponse};

/// Delete an existing user by ID (requires admin permissions)
/// 注意：此 handler 不注册为 Agent 工具（高危删除操作，仅管理员手动调用）。
#[generate_http_handler]
pub async fn delete_user(
    ctx: RequestContext,
    params: DeleteUserRequest,
) -> Result<DeleteUserResponse> {
    let domain = organization::domain();
    domain
        .user_manage()
        .delete_user(ctx, &params.user_id)
        .await?;

    Ok(DeleteUserResponse { success: true })
}
