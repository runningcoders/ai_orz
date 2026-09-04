//! Handler: POST /api/v1/organization/links/pairing/verify
//!
//! 验证配对码 + 交换凭证（机器侧，调用方是对端节点，无本地用户 JWT）。
//! 仅 `generate_http_handler`（不注册 Agent 工具）。在 router 中 root 层直挂，
//! 配对码本身鉴权，不进 `protected_routes` 的 JWT 链（评审稿 D7）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{VerifyPairingCodeRequest, VerifyPairingCodeResponse};
use common::error::Result;

/// 验证配对码 + 交换凭证（对端节点调用）
#[generate_http_handler]
pub async fn verify_pairing_code(
    ctx: RequestContext,
    params: VerifyPairingCodeRequest,
) -> Result<VerifyPairingCodeResponse> {
    organization::domain()
        .organization_manage()
        .verify_pairing_code(ctx, params)
        .await
}
