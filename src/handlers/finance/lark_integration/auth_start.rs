//! Handler: POST /api/v1/finance/identity/lark/auth/start - 发起 device flow 用户授权

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkAuthStartRequest, LarkAuthStartResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn auth_start(
    ctx: RequestContext,
    params: LarkAuthStartRequest,
) -> Result<LarkAuthStartResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let start = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_auth_start(ctx, &user_id, &params.domains)
        .await?;
    Ok(LarkAuthStartResponse {
        device_code: start.device_code,
        verification_url: start.verification_url,
        expires_in: start.expires_in,
    })
}
