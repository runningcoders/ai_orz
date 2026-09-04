//! Handler: POST /api/v1/organization/links
//!
//! 发起建联（用户侧，JWT）：凭对端配对码出站调对端 verify 完成双向凭证交换，
//! 落本地 link + Linked 影子。仅 `generate_http_handler`（不注册 Agent 工具，
//! 防 Agent 误触组网，评审稿 §4.2）。

use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{CreateLinkRequest, CreateLinkResponse};
use common::error::Result;

/// 发起建联（本地用户）
#[generate_http_handler]
pub async fn create_link(
    ctx: RequestContext,
    params: CreateLinkRequest,
) -> Result<CreateLinkResponse> {
    // 本端联邦地址：显式 public_base_url 优先，缺省由 listen_addr 推导。
    // Domain 不读全局配置单例（分层铁律），由 adapter 层解析后传入。
    let local_endpoint = crate::config::get().server.federation_base_url();

    organization::domain()
        .organization_manage()
        .create_link(ctx, params, local_endpoint)
        .await
}
