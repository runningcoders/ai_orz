//! Handler: GET /api/v1/finance/identity/wechat/qrcode/status - iLink 扫码状态长轮询
//!
//! 协议为**服务端有状态的长轮询**：同一 `qrcode` 反复查询即可，服务端 hold ~35s 无事件时
//! 返回 `wait`。因此前端"查询 → 处理 → 再查询"的紧循环天然就是轮询，无需额外间隔。
//!
//! 状态为官方 8 态，逐个的调用方动作：
//! - `wait` / `scaned`：继续轮询；
//! - `expired`：换新码（重取二维码）后继续；
//! - `need_verifycode`：取用手机微信展示的配对码，作为 `verify_code` 随轮询回传；
//! - `verify_code_blocked`：配对码连错被风控，需换新码；
//! - `scaned_but_redirect`：把响应的 `redirect_host` 作为参数回传，切换接入点；
//! - `confirmed`：凭据已自动落库并设为默认（终局）；
//! - `binded_redirect`：该 bot 早已绑过本客户端，幂等成功、未签发新凭据（终局）。

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{WechatLoginStatusRequest, WechatLoginStatusResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn login_status(
    ctx: RequestContext,
    params: WechatLoginStatusRequest,
) -> Result<WechatLoginStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.qrcode.trim().is_empty() {
        bail_err!(InvalidRequest, "qrcode 不能为空");
    }

    let outcome = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .wechat_login_poll(
            ctx,
            &user_id,
            &params.qrcode,
            params.verify_code.as_deref(),
            params.redirect_host.as_deref(),
        )
        .await?;

    let confirmed = outcome.credential_id.is_some();
    Ok(WechatLoginStatusResponse {
        status: outcome.status.as_str().to_string(),
        credential_id: outcome.credential_id,
        bot_id: outcome.bot_id,
        rotated: confirmed.then_some(outcome.rotated),
        user_id: outcome.user_id,
        bound_at: outcome.bound_at,
        redirect_host: outcome.redirect_host,
        already_bound: outcome.already_bound.then_some(true),
    })
}
