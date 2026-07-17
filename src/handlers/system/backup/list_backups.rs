//! Handler: GET /api/v1/system/backups - 列出所有备份。
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可访问。

use axum::{Json, extract::Extension};
use common::api::ApiResponse;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dal::backup::BackupInfo;
use crate::service::domain::system::domain;

pub async fn list_backups_handler(
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<ApiResponse<Vec<BackupInfo>>>> {
    let backups = domain()
        .backup_manager()
        .list_backups(ctx)
        .await?;

    Ok(Json(ApiResponse::success(backups)))
}
