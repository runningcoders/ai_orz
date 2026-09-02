//! Handler: POST /api/v1/system/processes/{pid}/kill - 终止后台进程（双露：HTTP + LLM 工具 shell_kill）

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ShellKillRequest, ShellKillResponse};
use common::error::Result;

/// 终止后台进程（SIGKILL）
#[register_handler_tool(
    id = "shell_kill",
    name = "Kill Shell Process",
    description = "Terminate a background process previously started via the shell_exec tool, by pid (SIGKILL). Returns the pid plus a killed flag that is false if the process had already exited. Fails with not found for unknown pids; agent callers may only kill processes they started themselves. Use shell_list to discover pids and shell_status to check a process before killing.",
    params = "common::api::ShellKillRequest",
    tags = "shell"
)]
#[generate_http_handler]
pub async fn shell_kill(
    ctx: RequestContext,
    params: ShellKillRequest,
) -> Result<ShellKillResponse> {
    let killed = system::domain()
        .process_manager()
        .kill_process(ctx, params.pid)?;

    Ok(ShellKillResponse {
        pid: params.pid,
        killed,
    })
}
