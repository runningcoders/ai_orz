//! POST /api/v1/system/seed/apply-default - 应用默认模板
//!
//! Handler 编排：加载内置默认 → 调用各 domain upsert

use ai_orz_macros::generate_http_handler;
use common::api::seed::{ApplyDefaultSeedRequest, LoadSeedResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::default;

#[generate_http_handler]
pub async fn apply_default(
    ctx: RequestContext,
    params: ApplyDefaultSeedRequest,
) -> Result<LoadSeedResponse> {
    super::check_super_admin(&ctx)?;
    let snapshot = default::embedded_default_snapshot();
    super::apply_snapshot_to_db(ctx, &snapshot, params.strategy, &params.sensitive_values).await
}
