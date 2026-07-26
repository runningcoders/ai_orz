//! Handler: POST /api/v1/system/backups - 创建数据备份。
//!
//! 仅 SuperAdmin 可调用（handler 内部二次校验）。
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可进入。

use ai_orz_macros::generate_http_handler;
use common::api::CreateBackupRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dal::backup::BackupInfo;
use crate::service::domain::system::domain;

use super::check_super_admin;

#[generate_http_handler]
pub async fn create_backup(
    ctx: RequestContext,
    _params: CreateBackupRequest,
) -> Result<BackupInfo> {
    check_super_admin(&ctx)?;

    let info = domain().backup_manager().create_backup(ctx).await?;

    Ok(info)
}
