//! Handler: POST /api/v1/organization/contracts/terminate
//!
//! 终止合约（用户侧，本端管理员 JWT，S3）：fail-closed 熔断开关，
//! 不删记录（保留审计线索）。仅 `generate_http_handler`（不注册 Agent 工具）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{TerminateContractRequest, TerminateContractResponse};
use common::error::Result;

/// 终止合约（管理员）
#[generate_http_handler]
pub async fn terminate_contract(
    ctx: RequestContext,
    params: TerminateContractRequest,
) -> Result<TerminateContractResponse> {
    organization::domain()
        .organization_manage()
        .terminate_contract(ctx, params)
        .await
}
