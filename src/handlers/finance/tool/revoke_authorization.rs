//! Handler: 工具授权单撤销（管理面·revoke，即时生效；终态不可撤销由 domain 保证）
//!
//! 撤销权归审批人面（user ctx）；Agent ctx 撤回本人申请单同走此口。
//! 撤销后该命令签名的拦截面恢复默认裁决（等效重新武装）。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AuthorizationDecisionResponse, RevokeAuthorizationRequest};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use crate::service::domain::finance::tool_authorization::status_to_dto;

/// 撤销授权单（即时生效：Pending/Active → Revoked）
#[register_handler_tool(
    id = "revoke_authorization",
    name = "Revoke Authorization",
    description = "Revoke a tool authorization record immediately (pending requests or active grants). Terminal-state records are rejected. Revocation re-arms the default interception behavior for the command signature.",
    params = "common::api::RevokeAuthorizationRequest",
    tags = "tool_management,admin"
)]
#[generate_http_handler]
pub async fn revoke_authorization(
    ctx: RequestContext,
    params: RevokeAuthorizationRequest,
) -> Result<AuthorizationDecisionResponse> {
    let authorization_id = params.authorization_id.clone();
    let outcome = domain()
        .tool_authorization_manage()
        .revoke_authorization(ctx, &authorization_id, params.reason)
        .await?;
    Ok(AuthorizationDecisionResponse {
        authorization_id: outcome.authorization_id,
        status: status_to_dto(outcome.status),
        grant_id: outcome.grant_id,
    })
}
