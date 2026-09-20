//! Handler: GET /api/v1/finance/identity/lark/bind/status - 绑定会话状态轮询
//!
//! 分支 B：done 时 secret 存 keychain 不可读，返回 done + 引导文案；
//! `app_id` 来自 CLI `--json` 输出的明文 appId（F17），前端预填、只补填 secret。

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
        // F17：CLI --json 输出的明文 appId（此前恒 None，契约字段从未被消费）
        app_id: snapshot.app_id,
        verification_url: snapshot.verification_url,
        error: snapshot.error,
    })
}
