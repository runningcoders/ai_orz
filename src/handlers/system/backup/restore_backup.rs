//! Handler: POST /api/v1/system/backups/{version}/restore - 获取恢复脚本。
//!
//! 仅 SuperAdmin 可调用（handler 内部二次校验）。
//! 返回纯文本 bash 脚本，content-type 为 text/plain。

use axum::{
    extract::{Extension, Path},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::domain;

use super::check_super_admin;

pub async fn restore_backup_handler(
    Extension(ctx): Extension<RequestContext>,
    Path(version): Path<u64>,
) -> Result<Response> {
    check_super_admin(&ctx)?;

    let script = domain()
        .backup_manager()
        .generate_restore_script(ctx, version)
        .await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        script,
    )
        .into_response())
}
