//! Handler: GET /api/v1/finance/identity/email/status?platform=qq - 邮箱机器人集成状态聚合
//!
//! platform 可选过滤：空串 = 返回全部提供商（渠道创建下拉需要跨平台拉取凭证）。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{EmailIntegrationStatusRequest, EmailIntegrationStatusResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn get_status(
    ctx: RequestContext,
    params: EmailIntegrationStatusRequest,
) -> Result<EmailIntegrationStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .email_bot_status(ctx, &user_id, params.platform.trim())
        .await
}
