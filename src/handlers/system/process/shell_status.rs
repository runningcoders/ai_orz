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
    name = "Check Shell Process Status",
    description = "Check one background process started via the shell_exec tool: returns the alive flag (refreshed), exit code, start time, command, log path, and the last lines of its output log (tail_lines, default 20, capped at 500). Fails with not found for unknown pids; agent callers may only inspect their own processes. Use shell_list to discover pids and shell_kill to stop one.",
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
