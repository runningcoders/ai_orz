//! Handler: POST /api/v1/finance/identity/lark/bind/start - 发起 config init --new 自动绑定

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkBindStartRequest, LarkBindStartResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn bind_start(
    ctx: RequestContext,
    _params: LarkBindStartRequest,
) -> Result<LarkBindStartResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let (session_id, verification_url) = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_bind_start(ctx, &user_id)
        .await?;
    Ok(LarkBindStartResponse {
        session_id,
        verification_url,
    })
}
