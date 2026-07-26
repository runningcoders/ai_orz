//! GET /api/v1/system/seed/file/{name} - 读取快照文件内容

use ai_orz_macros::generate_http_handler;
use common::api::seed::{GetSeedFileRequest, GetSeedFileResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn get_seed_file(
    _ctx: RequestContext,
    params: GetSeedFileRequest,
) -> Result<GetSeedFileResponse> {
    let dir = store::seeds_dir();
    store::read_file(&dir, &params.name).await
}
