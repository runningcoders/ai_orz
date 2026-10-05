//! tests 单元测试（拆分自 process.rs）
//!
//! 文件瘦身：原 229 行 → 95 行，测试体 135 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::system::new_for_test;
use common::enums::CallerType;

fn make_entry(pid: u32, agent_id: Option<&str>) -> ProcessEntry {
    ProcessEntry {
        pid,
        tool_id: "shell_exec".to_string(),
        call_id: format!("call-{}", pid),
        agent_id: agent_id.map(|s| s.to_string()),
        project_id: None,
        task_id: None,
        command: "sleep 30".to_string(),
        working_dir: "/tmp".to_string(),
        log_path: format!("/tmp/{}.log", pid),
        background: true,
        started_at: 0,
        status: process::ProcessStatus::Running,
        exit_code: None,
        finished_at: None,
    }
}

fn agent_ctx(agent_id: &str) -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let base = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    base.to_builder()
        .caller_type(CallerType::Agent)
        .agent_id(agent_id)
        .build()
}

fn user_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    crate::pkg::request_context_test_support::new_test_ctx("user-1", pool)
}

fn test_domain() -> std::sync::Arc<dyn crate::service::domain::system::SystemDomain> {
    new_for_test()
}

#[tokio::test]
async fn test_scope_agent_mismatch_denied() {
    let domain = test_domain();
    process::registry().register(make_entry(91001, Some("agent-a")));

    let result = domain
        .process_manager()
        .get_process(agent_ctx("agent-b"), 91001);
    assert!(result.is_err());

    process::registry().remove(91001);
}

#[tokio::test]
async fn test_scope_agent_match_allowed() {
    let domain = test_domain();
    process::registry().register(make_entry(91002, Some("agent-a")));

    let entry = domain
        .process_manager()
        .get_process(agent_ctx("agent-a"), 91002)
        .expect("matching agent should pass scope check");
    assert_eq!(entry.pid, 91002);

    process::registry().remove(91002);
}

#[tokio::test]
async fn test_scope_user_ctx_allowed() {
    let domain = test_domain();
    process::registry().register(make_entry(91003, Some("agent-a")));

    let entry = domain
        .process_manager()
        .get_process(user_ctx(), 91003)
        .expect("human user ctx should pass scope check");
    assert_eq!(entry.pid, 91003);

    process::registry().remove(91003);
}

#[tokio::test]
async fn test_list_processes_agent_filtered() {
    let domain = test_domain();
    process::registry().register(make_entry(91004, Some("agent-a")));
    process::registry().register(make_entry(91005, Some("agent-b")));

    let agent_list = domain
        .process_manager()
        .list_processes(agent_ctx("agent-a"))
        .unwrap();
    assert!(
        agent_list
            .iter()
            .all(|e| e.agent_id.as_deref() == Some("agent-a"))
    );

    let user_list = domain.process_manager().list_processes(user_ctx()).unwrap();
    assert!(user_list.len() >= 2);

    process::registry().remove(91004);
    process::registry().remove(91005);
}

#[cfg(unix)]
#[tokio::test]
async fn test_kill_process_real() {
    let domain = test_domain();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id();

    let mut entry = make_entry(pid, Some("agent-a"));
    entry.started_at = common::constants::utils::current_timestamp_ms() as u64;
    process::registry().register(entry);

    let killed = domain
        .process_manager()
        .kill_process(agent_ctx("agent-a"), pid)
        .expect("kill by owner agent should succeed");
    assert!(killed);
    let _ = child.wait();

    let status = domain
        .process_manager()
        .get_process(user_ctx(), pid)
        .unwrap();
    assert_eq!(status.status, process::ProcessStatus::Exited);

    process::registry().remove(pid);
}
