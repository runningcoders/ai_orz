//! Handler: GET /api/v1/finance/identity/wechat/qrcode/status - iLink 扫码状态长轮询
//!
//! 前端轮询节奏：1s 间隔持续调用；服务端无新事件时会 hold ~35s 才返回 wait，
//! 请求超时（>45s）属异常应重试。confirmed 时凭据已自动落库并设为默认。

use crate::pkg::RequestContext;
use crate::pkg::wechat_ilink::IlinkQrStatusKind;
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
        .wechat_login_poll(ctx, &user_id, &params.qrcode)
        .await?;

    let status = match outcome.status {
        IlinkQrStatusKind::Wait => "wait",
        IlinkQrStatusKind::Scaned => "scaned",
        IlinkQrStatusKind::Expired => "expired",
        IlinkQrStatusKind::Confirmed => "confirmed",
    };
    let confirmed = outcome.credential_id.is_some();
    Ok(WechatLoginStatusResponse {
        status: status.to_string(),
        credential_id: outcome.credential_id,
        bot_id: outcome.bot_id,
        rotated: if confirmed {
            Some(outcome.rotated)
        } else {
            None
        },
    })
}
