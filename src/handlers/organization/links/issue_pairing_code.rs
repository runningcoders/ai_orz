//! Handler: POST /api/v1/organization/links/pairing/issue
//!
//! 签发组网配对码（用户侧，需本组织管理员权限）。
//! 仅 `generate_http_handler`（不注册 Agent 工具，防 Agent 误触组网，评审稿 §4.2）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{IssuePairingCodeRequest, IssuePairingCodeResponse};
use common::error::Result;

/// 签发组网配对码（本端管理员）
#[generate_http_handler]
pub async fn issue_pairing_code(
    ctx: RequestContext,
    _params: IssuePairingCodeRequest,
) -> Result<IssuePairingCodeResponse> {
    organization::domain()
        .organization_manage()
        .issue_pairing_code(ctx)
        .await
}
