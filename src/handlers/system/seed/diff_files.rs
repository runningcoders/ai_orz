//! POST /api/v1/system/seed/diff-files - 两个文件之间 diff
//!
//! 纯文件对比，不涉及 DB

use ai_orz_macros::generate_http_handler;
use common::api::seed::DiffFilesRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{
    defs::{DiffKind, SeedDiff, SeedSnapshot},
    diff, store,
};

#[generate_http_handler]
pub async fn diff_files(_ctx: RequestContext, params: DiffFilesRequest) -> Result<SeedDiff> {
    let dir = store::seeds_dir();
    let base_resp = store::read_file(&dir, &params.base).await?;
    let target_resp = store::read_file(&dir, &params.target).await?;
    let base_snapshot: SeedSnapshot = serde_json::from_str(&base_resp.content)?;
    let target_snapshot: SeedSnapshot = serde_json::from_str(&target_resp.content)?;

    let mut diff_result = diff::diff_snapshots(&base_snapshot, &target_snapshot);
    diff_result.meta.kind = DiffKind::FileVsFile;
    diff_result.meta.base_source = params.base;
    diff_result.meta.target_source = params.target;
    Ok(diff_result)
}
