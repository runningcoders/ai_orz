//! Handler: POST /api/v1/system/storage/tool-logs/cleanup - 手动清理工具日志
//!
//! 与 cron 系统任务（tool_log_cleanup action）共用同一清理入口（pkg::tool_log_retention::cleanup_tool_logs），
//! 共享 Running 进程日志保护逻辑；请求可携带 retention_days 覆盖配置值（缺省读 [tool_log].retention_days）。

use ai_orz_macros::generate_http_handler;
use common::api::{CleanupToolLogsRequest, CleanupToolLogsResponse};
use common::error::Result;

use crate::config;
use crate::pkg::RequestContext;

#[generate_http_handler]
pub async fn cleanup_tool_logs(
    _ctx: RequestContext,
    params: CleanupToolLogsRequest,
) -> Result<CleanupToolLogsResponse> {
    let cfg = config::get();
    let retention_days = params.retention_days.unwrap_or(cfg.tool_log.retention_days);

    // retention = 0：清理关闭，空跑返回（success = false 表示未执行删除）
    if retention_days == 0 {
        return Ok(CleanupToolLogsResponse {
            success: false,
            retention_days,
            removed_dirs: 0,
            removed_files: 0,
            freed_bytes: 0,
            skipped_dirs: 0,
        });
    }

    let base_path = cfg.base_data_path();
    let report = tokio::task::spawn_blocking(move || {
        crate::pkg::tool_log_retention::cleanup_tool_logs(&base_path, retention_days)
    })
    .await
    .map_err(|e| common::error::Error::internal(format!("tool log cleanup join error: {}", e)))?;

    Ok(CleanupToolLogsResponse {
        success: true,
        retention_days,
        removed_dirs: report.removed_dirs as u64,
        removed_files: report.removed_files as u64,
        freed_bytes: report.freed_bytes,
        skipped_dirs: report.skipped_dirs as u64,
    })
}
