//! Handler: 工具授权单查询（管理面·list）
//!
//! 归属基准强制（不信任客户端传值越权查询）：
//! - Agent ctx：强制 agent_id=ctx.agent_id（仅本人申请单，防横向越权）；
//! - user ctx：强制 user_id=ctx.uid()（审批人视角，按归属用户全量）。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AuthorizationDetailDto, AuthorizationQueryRequest};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 授权单列表（状态/申请人/工具过滤；归属基准由 ctx 强制注入）
#[register_handler_tool(
    id = "list_authorizations",
    name = "List Authorizations",
    description = "List tool authorization records with optional status/tool filters. Agent contexts can only list their own requests (agent_id is forced from context); user contexts are scoped to records they own as approver.",
    params = "common::api::AuthorizationQueryRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn list_authorizations(
    ctx: RequestContext,
    mut params: AuthorizationQueryRequest,
) -> Result<Vec<AuthorizationDetailDto>> {
    if let Some(aid) = ctx.agent_id.clone() {
        params.agent_id = Some(aid);
    } else {
        params.user_id = Some(ctx.uid());
    }
    domain()
        .tool_authorization_manage()
        .list_authorizations(ctx, params)
        .await
}
