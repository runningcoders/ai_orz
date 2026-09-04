//! Handler: GET /api/v1/organization/links/federation-agents
//!
//! 联邦 Agent 目录（用户侧，JWT）：聚合所有 Active 连接对端开放的可调用
//! Agent，mention picker 联邦候选数据源（P5）。
//! 仅 `generate_http_handler`（不注册 Agent 工具，防 Agent 误触组网，评审稿 §4.2）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{ListFederationAgentsRequest, ListFederationAgentsResponse};
use common::error::Result;

/// 联邦 Agent 目录（本地用户）
#[generate_http_handler]
pub async fn list_federation_agents(
    ctx: RequestContext,
    _params: ListFederationAgentsRequest,
) -> Result<ListFederationAgentsResponse> {
    organization::domain()
        .organization_manage()
        .list_federation_agents(ctx)
        .await
}
