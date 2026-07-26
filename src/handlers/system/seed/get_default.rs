//! GET /api/v1/system/seed/default - 获取内置默认模板

use ai_orz_macros::generate_http_handler;
use common::api::seed::GetDefaultSeedRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{default, defs::SeedSnapshot};

#[generate_http_handler]
pub async fn get_default(
    _ctx: RequestContext,
    _params: GetDefaultSeedRequest,
) -> Result<SeedSnapshot> {
    Ok(default::embedded_default_snapshot())
}
