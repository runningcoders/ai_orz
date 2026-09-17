//! Handler: POST /api/v1/organization/me/invite-code/regenerate
//!
//! 轮换当前组织邀请码：无条件签发新码，旧码立即失效。仅组织管理员可调用
//! （路由层门控）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{InviteCodeResponse, RegenerateInviteCodeRequest};
use common::error::Result;

/// 轮换当前组织邀请码（管理员）
#[generate_http_handler]
pub async fn regenerate_invite_code(
    ctx: RequestContext,
    _params: RegenerateInviteCodeRequest,
) -> Result<InviteCodeResponse> {
    let invite_code = organization::domain()
        .organization_manage()
        .rotate_invite_code(ctx)
        .await?;
    Ok(InviteCodeResponse { invite_code })
}
