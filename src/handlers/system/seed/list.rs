//! GET /api/v1/system/seed/list - 列出 seeds/ 目录

use ai_orz_macros::generate_http_handler;
use common::api::seed::{ListSeedsRequest, ListSeedsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn list_seeds(
    _ctx: RequestContext,
    _params: ListSeedsRequest,
) -> Result<ListSeedsResponse> {
    let dir = store::seeds_dir();
    let files = store::list_files(&dir).await?;
    let total = files.len() as u64;
    Ok(ListSeedsResponse { data: files, total })
}
