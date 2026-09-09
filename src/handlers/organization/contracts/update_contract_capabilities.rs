//! Handler: PUT /api/v1/organization/contracts/capabilities
//!
//! 更新合约能力集（用户侧，本端管理员 JWT，S3）。
//! 下一次入站请求即按新能力集判定。仅 `generate_http_handler`（不注册 Agent 工具）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateContractCapabilitiesRequest, UpdateContractCapabilitiesResponse};
use common::error::Result;

/// 更新合约能力集（管理员）
#[generate_http_handler]
pub async fn update_contract_capabilities(
    ctx: RequestContext,
    params: UpdateContractCapabilitiesRequest,
) -> Result<UpdateContractCapabilitiesResponse> {
    organization::domain()
        .organization_manage()
        .update_contract_capabilities(ctx, params)
        .await
}
