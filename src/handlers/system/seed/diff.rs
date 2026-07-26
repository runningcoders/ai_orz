//! POST /api/v1/system/seed/diff/{name} - 文件 vs DB diff
//!
//! Handler 编排：读文件 → 调用各 domain 拉当前 DB → 组装 current snapshot → 调用 seed::diff_snapshots

use ai_orz_macros::generate_http_handler;
use common::api::seed::DiffSeedRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{
    defs::{DiffKind, SeedDiff, SeedSnapshot},
    diff, store,
};

#[generate_http_handler]
pub async fn diff(ctx: RequestContext, params: DiffSeedRequest) -> Result<SeedDiff> {
    let dir = store::seeds_dir();
    let file_resp = store::read_file(&dir, &params.name).await?;
    let snapshot: SeedSnapshot = serde_json::from_str(&file_resp.content)?;

    // 编排各 domain 拉取当前 DB
    let current =
        super::assemble_snapshot_from_db(ctx, &snapshot.source_organization_id, None).await?;

    let mut diff_result = diff::diff_snapshots(&current, &snapshot);
    diff_result.meta.kind = DiffKind::FileVsDb;
    diff_result.meta.base_source = "current_db".to_string();
    diff_result.meta.target_source = params.name;
    Ok(diff_result)
}
