//! Handler: POST /api/v1/finance/identity/lark/auth/logout - 取消用户授权

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkAuthLogoutRequest, LarkAuthLogoutResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn auth_logout(
    ctx: RequestContext,
    _params: LarkAuthLogoutRequest,
) -> Result<LarkAuthLogoutResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let outcome = match crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_auth_logout(ctx, &user_id)
        .await
    {
        Ok(o) => o,
        Err(e) => crate::pkg::lark_integration::LarkAuthOutcome {
            success: false,
            hint: Some(e.to_string()),
            ..Default::default()
        },
    };
    Ok(LarkAuthLogoutResponse {
        success: outcome.success,
        degraded: outcome.degraded,
        hint: outcome.hint,
    })
}
