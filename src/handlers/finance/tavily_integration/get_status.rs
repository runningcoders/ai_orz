//! Handler: GET /api/v1/finance/identity/tavily/status - Tavily 集成状态聚合

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{TavilyIntegrationStatusRequest, TavilyIntegrationStatusResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn get_status(
    ctx: RequestContext,
    _params: TavilyIntegrationStatusRequest,
) -> Result<TavilyIntegrationStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .tavily_integration_status(ctx, &user_id)
        .await
}
