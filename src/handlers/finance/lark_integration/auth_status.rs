//! Handler: GET /api/v1/finance/identity/lark/auth/status - 用户授权状态

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkAuthStatusRequest, LarkAuthStatusResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn auth_status(
    ctx: RequestContext,
    _params: LarkAuthStatusRequest,
) -> Result<LarkAuthStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 前置条件不满足（未绑定/无 CLI）时降级为未授权 + 引导提示，不抛 500
    let status = match crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_auth_status(ctx, &user_id)
        .await
    {
        Ok(s) => s,
        Err(e) => crate::pkg::lark_integration::LarkAuthStatus {
            hint: Some(e.to_string()),
            ..Default::default()
        },
    };
    Ok(LarkAuthStatusResponse {
        logged_in: status.logged_in,
        user_name: status.user_name,
        degraded: status.degraded,
        hint: status.hint,
    })
}
