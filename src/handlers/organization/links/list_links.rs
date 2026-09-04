//! Handler: GET /api/v1/organization/links
//!
//! 已建联列表（用户侧，JWT）：前端"关联组织"页数据源。
//! 仅 `generate_http_handler`（不注册 Agent 工具，防 Agent 误触组网，评审稿 §4.2）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{ListLinksRequest, ListLinksResponse};
use common::error::Result;

/// 已建联列表（本地用户）
#[generate_http_handler]
pub async fn list_links(
    ctx: RequestContext,
    _params: ListLinksRequest,
) -> Result<ListLinksResponse> {
    organization::domain()
        .organization_manage()
        .list_links(ctx)
        .await
}
