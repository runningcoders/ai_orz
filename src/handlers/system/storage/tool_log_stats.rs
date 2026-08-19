//! Handler: GET /api/v1/system/storage/tool-logs - 工具日志存储统计
//!
//! 系统监控页「存储」维度数据源：① 运行时输出层的占用统计（按天分布）+ 当前保留策略。
//! 目录约定：{base}/tools/{tool_id}/logs/{YYYYMMDD}/{call_id}.log

use ai_orz_macros::generate_http_handler;
use common::api::{GetToolLogStorageRequest, ToolLogDayStatItem, ToolLogStorageResponse};
use common::error::Result;

use crate::config;
use crate::pkg::RequestContext;

#[generate_http_handler]
pub async fn get_tool_log_storage(
    _ctx: RequestContext,
    _params: GetToolLogStorageRequest,
) -> Result<ToolLogStorageResponse> {
    let cfg = config::get();
    let base_path = cfg.base_data_path();
    let retention_days = cfg.tool_log.retention_days;

    let stats = tokio::task::spawn_blocking(move || {
        crate::pkg::tool_log_retention::tool_log_storage_stats(&base_path)
    })
    .await
    .map_err(|e| common::error::Error::internal(format!("tool log stats join error: {}", e)))?;

    Ok(ToolLogStorageResponse {
        total_bytes: stats.total_bytes,
        total_files: stats.total_files as u64,
        by_day: stats
            .by_day
            .into_iter()
            .map(|d| ToolLogDayStatItem {
                day: d.day,
                files: d.files as u64,
                bytes: d.bytes,
            })
            .collect(),
        retention_days,
    })
}
