//! Handler: GET /api/v1/finance/identity/generic-token/status?platform=xxx - 通用 API Token 集成状态聚合

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{
    GenericTokenIntegrationStatusRequest, GenericTokenIntegrationStatusResponse,
};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn get_status(
    ctx: RequestContext,
    params: GenericTokenIntegrationStatusRequest,
) -> Result<GenericTokenIntegrationStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    let platform = params.platform.trim();
    if platform.is_empty() {
        bail_err!(InvalidRequest, "platform 查询参数不能为空");
    }

    domain()
        .identity_credential_manage()
        .generic_token_status(ctx, &user_id, platform)
        .await
}
