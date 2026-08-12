//! Handler: GET /api/v1/finance/identity/lark/bind/status - 绑定会话状态轮询
//!
//! 分支 B：done 时 secret 不可读出（存于 keychain），返回 done + 引导文案，
//! 前端引导用户去飞书集成手动补填凭证（app_id 亦不在此返回，避免误读配置）。

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{LarkBindStatusRequest, LarkBindStatusResponse};
use common::error::{Result, bail_err, err};

#[generate_http_handler]
pub async fn bind_status(
    ctx: RequestContext,
    params: LarkBindStatusRequest,
) -> Result<LarkBindStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.session_id.trim().is_empty() {
        bail_err!(InvalidRequest, "session_id 不能为空");
    }

    let snapshot = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .lark_bind_status(ctx, &user_id, &params.session_id)
        .await?
        .ok_or_else(|| {
            err!(
                NotFound,
                "绑定会话不存在或已过期 session_id={}",
                params.session_id
            )
        })?;

    Ok(LarkBindStatusResponse {
        status: snapshot.phase.as_str().to_string(),
        credential_id: None,
        channel_id: None,
        app_id: None,
        verification_url: snapshot.verification_url,
        error: snapshot.error,
    })
}
