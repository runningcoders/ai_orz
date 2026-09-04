//! Handler: DELETE /api/v1/organization/links/{peer_org_id}
//!
//! 断联（用户侧，本端管理员 JWT）：连接置 Revoked，不删除对端影子记录
//! （保留历史审计线索）。仅 `generate_http_handler`（不注册 Agent 工具，
//! 防 Agent 误触组网，评审稿 §4.2）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{RevokeLinkRequest, RevokeLinkResponse};
use common::error::Result;

/// 断联（本端管理员）
#[generate_http_handler]
pub async fn revoke_link(
    ctx: RequestContext,
    params: RevokeLinkRequest,
) -> Result<RevokeLinkResponse> {
    organization::domain()
        .organization_manage()
        .revoke_link(ctx, &params.peer_org_id)
        .await?;

    Ok(RevokeLinkResponse { success: true })
}
