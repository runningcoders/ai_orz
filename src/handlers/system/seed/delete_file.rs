//! DELETE /api/v1/system/seed/file/{name} - 删除快照文件

use ai_orz_macros::generate_http_handler;
use common::api::seed::{DeleteSeedFileRequest, DeleteSeedFileResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn delete_seed_file(
    ctx: RequestContext,
    params: DeleteSeedFileRequest,
) -> Result<DeleteSeedFileResponse> {
    super::check_super_admin(&ctx)?;
    let dir = store::seeds_dir();
    store::delete_file(&dir, &params.name).await?;
    Ok(DeleteSeedFileResponse { success: true })
}
