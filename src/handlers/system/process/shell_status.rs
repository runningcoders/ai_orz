//! Handler: GET /api/v1/system/processes/{pid} - 查询后台进程状态（双露：HTTP + LLM 工具 shell_status）

use crate::pkg::RequestContext;
use crate::pkg::process::ProcessStatus;
use crate::service::domain::system;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ShellStatusRequest, ShellStatusResponse};
use common::error::Result;

/// 查询后台进程状态（探活刷新 + 日志尾部）
#[register_handler_tool(
    id = "shell_status",
    name = "shell_status",
    description = "Query the status of a background process started by shell_exec (alive check, exit code, and log tail). Use the pid returned by shell_exec.",
    params = "common::api::ShellStatusRequest",
    tags = "shell"
)]
#[generate_http_handler]
pub async fn shell_status(
    ctx: RequestContext,
    params: ShellStatusRequest,
) -> Result<ShellStatusResponse> {
    let detail =
        system::domain()
            .process_manager()
            .process_status(ctx, params.pid, params.tail_lines)?;

    Ok(ShellStatusResponse {
        pid: detail.entry.pid,
        alive: detail.entry.status == ProcessStatus::Running,
        exit_code: detail.entry.exit_code,
        started_at: detail.entry.started_at,
        command: detail.entry.command,
        log_path: detail.entry.log_path,
        call_id: detail.entry.call_id,
        log_tail: detail.log_tail,
    })
}
