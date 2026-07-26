//! POST /api/v1/system/seed/load/{name} - 从文件加载快照
//!
//! Handler 编排：读文件 → 校验 → 调用各 domain upsert

use ai_orz_macros::generate_http_handler;
use common::api::seed::{LoadSeedRequest, LoadSeedResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn load_seed(
    ctx: RequestContext,
    params: LoadSeedRequest,
) -> Result<LoadSeedResponse> {
    super::check_super_admin(&ctx)?;

    let dir = store::seeds_dir();
    let file_resp = store::read_file(&dir, &params.name).await?;
    let snapshot: crate::service::domain::system::seed::defs::SeedSnapshot =
        serde_json::from_str(&file_resp.content)?;

    super::apply_snapshot_to_db(ctx, &snapshot, params.strategy, &params.sensitive_values).await
}
