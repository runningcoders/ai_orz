//! Handler: GET /api/v1/organization/contracts
//!
//! 本组织的联邦合约列表（用户侧，本端管理员 JWT，S3）。
//! 仅 `generate_http_handler`（不注册 Agent 工具）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{ListContractsRequest, ListContractsResponse};
use common::error::Result;

/// 本组织的合约列表（管理员）
#[generate_http_handler]
pub async fn list_contracts(
    ctx: RequestContext,
    _params: ListContractsRequest,
) -> Result<ListContractsResponse> {
    organization::domain()
        .organization_manage()
        .list_contracts(ctx)
        .await
}
