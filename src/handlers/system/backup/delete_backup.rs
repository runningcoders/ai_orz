//! Handler: DELETE /api/v1/system/backups/{version} - 删除指定版本的备份。
//!
//! 仅 SuperAdmin 可调用（handler 内部二次校验）。

use ai_orz_macros::generate_http_handler;
use common::api::DeleteBackupRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::check_super_admin;

#[generate_http_handler]
pub async fn delete_backup(
    ctx: RequestContext,
    params: DeleteBackupRequest,
) -> Result<()> {
    check_super_admin(&ctx)?;

    domain()
        .backup_manager()
        .delete_backup(ctx, params.version)
        .await?;

    Ok(())
}
