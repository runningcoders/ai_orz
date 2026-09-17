//! Handler: GET /api/v1/organization/me/invite-code
//!
//! 获取当前组织邀请码：从未签发时懒生成并持久化（首次查看即签发），
//! 已有有效码则原样返回。仅组织管理员可调用（路由层门控）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{GetInviteCodeRequest, InviteCodeResponse};
use common::error::Result;

/// 获取当前组织邀请码（管理员，懒生成）
#[generate_http_handler]
pub async fn get_invite_code(
    ctx: RequestContext,
    _params: GetInviteCodeRequest,
) -> Result<InviteCodeResponse> {
    let invite_code = organization::domain()
        .organization_manage()
        .get_or_create_invite_code(ctx)
        .await?;
    Ok(InviteCodeResponse { invite_code })
}
