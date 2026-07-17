//! Handler: DELETE /api/v1/system/backups/{version} - 删除指定版本的备份。
//!
//! 仅 SuperAdmin 可调用（handler 内部二次校验）。

use axum::{Json, extract::{Extension, Path}};
use common::api::ApiResponse;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::check_super_admin;

pub async fn delete_backup_handler(
    Extension(ctx): Extension<RequestContext>,
    Path(version): Path<u64>,
) -> Result<Json<ApiResponse<()>>> {
    check_super_admin(&ctx)?;

    domain()
        .backup_manager()
        .delete_backup(ctx, version)
        .await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
