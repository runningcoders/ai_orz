//! Handler: GET /api/v1/system/backups - 列出所有备份。
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可访问。

use ai_orz_macros::generate_http_handler;
use common::api::ListBackupsRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dal::backup::BackupInfo;
use crate::service::domain::system::domain;

#[generate_http_handler]
pub async fn list_backups(
    ctx: RequestContext,
    _params: ListBackupsRequest,
) -> Result<Vec<BackupInfo>> {
    let backups = domain().backup_manager().list_backups(ctx).await?;

    Ok(backups)
}
