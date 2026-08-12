//! Handler: POST /api/v1/finance/identity/lark/auth/complete - 以 device_code 完成授权

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkAuthCompleteRequest, LarkAuthCompleteResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn auth_complete(
    ctx: RequestContext,
    params: LarkAuthCompleteRequest,
) -> Result<LarkAuthCompleteResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.device_code.trim().is_empty() {
        bail_err!(InvalidRequest, "device_code 不能为空");
    }

    let outcome = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_auth_complete(ctx, &user_id, &params.device_code)
        .await?;
    Ok(LarkAuthCompleteResponse {
        success: outcome.success,
        degraded: outcome.degraded,
        hint: outcome.hint,
    })
}
