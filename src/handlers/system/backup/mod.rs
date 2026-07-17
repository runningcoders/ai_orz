//! Backup 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod create_backup;
pub mod delete_backup;
pub mod list_backups;
pub mod restore_backup;

pub use create_backup::create_backup_handler;
pub use delete_backup::delete_backup_handler;
pub use list_backups::list_backups_handler;
pub use restore_backup::restore_backup_handler;

use common::enums::UserRole;
use common::error::{Error, Result};

use crate::pkg::RequestContext;

/// 校验当前用户是否为 SuperAdmin
///
/// 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可进入，
/// 此函数在 handler 内部对创建/删除/恢复等高危操作做二次校验，仅放行 SuperAdmin。
fn check_super_admin(ctx: &RequestContext) -> Result<()> {
    let user_role = ctx
        .user_role()
        .map(UserRole::from_i32)
        .unwrap_or(UserRole::Member);
    if !UserRole::has_permission(user_role, UserRole::SuperAdmin) {
        return Err(Error::forbidden("权限不足，仅 SuperAdmin 可执行此操作"));
    }
    Ok(())
}
