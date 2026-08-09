//! Handler: POST /api/v1/system/processes/{pid}/kill - 终止后台进程（双露：HTTP + LLM 工具 shell_kill）

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ShellKillRequest, ShellKillResponse};
use common::error::Result;

/// 终止后台进程（SIGKILL）
#[register_handler_tool(
    id = "shell_kill",
    name = "shell_kill",
    description = "Terminate a background process started by shell_exec (SIGKILL). Use the pid returned by shell_exec or shell_status.",
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
