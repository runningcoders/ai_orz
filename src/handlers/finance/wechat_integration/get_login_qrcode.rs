//! Handler: POST /api/v1/finance/identity/wechat/qrcode - 获取 iLink 登录二维码

use crate::pkg::RequestContext;
use ai_orz_macros::generate_http_handler;
use common::api::{WechatLoginQrcodeRequest, WechatLoginQrcodeResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn get_login_qrcode(
    ctx: RequestContext,
    _params: WechatLoginQrcodeRequest,
) -> Result<WechatLoginQrcodeResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let qr = crate::service::domain::finance::domain()
        .identity_credential_manage()
        .wechat_login_qrcode(ctx)
        .await?;
    Ok(WechatLoginQrcodeResponse {
        qrcode: qr.qrcode,
        qrcode_img_content: qr.qrcode_img_content,
    })
}
