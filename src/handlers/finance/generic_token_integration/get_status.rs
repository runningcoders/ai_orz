//! Handler: GET /api/v1/finance/identity/generic-token/status?platform=xxx - 通用 API Token 集成状态聚合
//!
//! platform 可选过滤：空串（或不传）= 返回该用户全部 GenericToken 凭证
//! （自定义凭证区块全量表格数据源，沿 email/status「空串=全量」同构口径）。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{GenericTokenIntegrationStatusRequest, GenericTokenIntegrationStatusResponse};
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
    domain()
        .identity_credential_manage()
        .generic_token_status(ctx, &user_id, params.platform.trim())
        .await
}
