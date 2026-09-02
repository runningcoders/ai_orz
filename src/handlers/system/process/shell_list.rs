//! Handler: GET /api/v1/system/processes - 列出后台进程（双露：HTTP + LLM 工具 shell_list）

use crate::pkg::RequestContext;
use crate::pkg::process::ProcessStatus;
use crate::service::domain::system;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListProcessesRequest, ListProcessesResponse, ProcessInfo};
use common::error::Result;

/// 列出后台进程（Agent 调用方仅可见自己启动的；逐条探活刷新状态）
#[register_handler_tool(
    id = "shell_list",
    name = "List Shell Processes",
    description = "List background processes started via the shell_exec tool, oldest first, each with pid, command, working directory, start time, alive flag (refreshed), exit code, and output log path. Takes no parameters. Agent callers only see their own processes; human callers see all. Use shell_status for one process's log tail or shell_kill to stop one.",
    params = "common::api::ListProcessesRequest",
    tags = "shell"
)]
#[generate_http_handler]
pub async fn shell_list(
    ctx: RequestContext,
    _params: ListProcessesRequest,
) -> Result<ListProcessesResponse> {
    let entries = system::domain().process_manager().list_processes(ctx)?;

    let processes = entries
        .into_iter()
        .map(|entry| {
            // 探活刷新：Running 但进程已不存在 → 标记 Exited
            let entry = crate::pkg::process::registry()
                .refresh(entry.pid)
                .unwrap_or(entry);
            ProcessInfo {
                pid: entry.pid,
                call_id: entry.call_id,
                tool_id: entry.tool_id,
                agent_id: entry.agent_id,
                command: entry.command,
                working_dir: entry.working_dir,
                background: entry.background,
                started_at: entry.started_at,
                alive: entry.status == ProcessStatus::Running,
                exit_code: entry.exit_code,
                log_path: entry.log_path,
            }
        })
        .collect();

    Ok(ListProcessesResponse { processes })
}

#[cfg(test)]
mod tests {
    use super::shell_list;
    use crate::pkg::process::{self, ProcessEntry, ProcessStatus};
    use common::api::ListProcessesRequest;
    use sqlx::SqlitePool;
    use std::sync::Once;

    fn init_test_singletons() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let base_path = std::env::temp_dir().join(format!(
                "ai_orz_shell_list_handler_tests_{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&base_path)
                .expect("handler shell_list trace base path should be created");
            crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

            let _ = crate::config::init();
            crate::service::dao::init_all();
            crate::service::dal::init_all();
            crate::service::domain::init_all();
        });
    }

    fn user_ctx() -> crate::pkg::RequestContext {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        crate::pkg::request_context_test_support::new_test_ctx("shell-list-user", pool)
    }

    fn agent_ctx(agent_id: &str) -> crate::pkg::RequestContext {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        crate::pkg::request_context_test_support::new_test_ctx("shell-list-user", pool)
            .to_builder()
            .caller_type(common::enums::CallerType::Agent)
            .agent_id(agent_id.to_string())
            .build()
    }

    fn make_entry(pid: u32, agent_id: Option<&str>) -> ProcessEntry {
        ProcessEntry {
            pid,
            tool_id: "shell_exec".to_string(),
            call_id: format!("call-{}", pid),
            agent_id: agent_id.map(ToString::to_string),
            project_id: None,
            task_id: None,
            command: format!("sleep {}", pid),
            working_dir: "/tmp".to_string(),
            log_path: format!("/tmp/{}.log", pid),
            background: true,
            started_at: pid as u64,
            status: ProcessStatus::Exited,
            exit_code: Some(0),
            finished_at: Some(pid as u64 + 1),
        }
    }

    #[tokio::test]
    async fn shell_list_user_ctx_sees_all_processes() {
        init_test_singletons();
        process::registry().register(make_entry(92001, Some("agent-x")));
        process::registry().register(make_entry(92002, None));

        let resp = shell_list(user_ctx(), ListProcessesRequest {})
            .await
            .expect("user ctx should list all processes");
        let pids: Vec<u32> = resp.processes.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&92001));
        assert!(pids.contains(&92002));

        process::registry().remove(92001);
        process::registry().remove(92002);
    }

    #[tokio::test]
    async fn shell_list_agent_ctx_sees_only_own_processes() {
        init_test_singletons();
        process::registry().register(make_entry(92003, Some("agent-a")));
        process::registry().register(make_entry(92004, Some("agent-b")));

        let resp = shell_list(agent_ctx("agent-a"), ListProcessesRequest {})
            .await
            .expect("agent ctx should list own processes");
        let pids: Vec<u32> = resp.processes.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&92003));
        assert!(!pids.contains(&92004));

        process::registry().remove(92003);
        process::registry().remove(92004);
    }
}
