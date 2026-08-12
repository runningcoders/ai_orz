//! Handler: POST /api/v1/finance/identity/lark/bind/cancel - 取消绑定会话

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkBindCancelRequest, LarkBindCancelResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn bind_cancel(
    ctx: RequestContext,
    params: LarkBindCancelRequest,
) -> Result<LarkBindCancelResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.session_id.trim().is_empty() {
        bail_err!(InvalidRequest, "session_id 不能为空");
    }

    let success = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_bind_cancel(ctx, &user_id, &params.session_id)
        .await?;
    Ok(LarkBindCancelResponse { success })
}
