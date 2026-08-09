//! ProcessManager 子模块实现：统一后台进程管理（带 Agent scope 校验）
//!
//! scope 规则：
//! - `ctx.agent_id()` 为 Some 时必须与 entry.agent_id 匹配（Agent 只能管理自己启动的进程）
//! - ctx 无 agent_id（人类用户/管理面调用）放行

use crate::pkg::process::{self, ProcessEntry};
use crate::pkg::request_context::RequestContext;
use common::error::Result;

use super::SystemDomainImpl;

/// 进程状态详情（注册中心条目 + 日志尾部）
#[derive(Debug, Clone)]
pub struct ProcessStatusDetail {
    pub entry: ProcessEntry,
    pub log_tail: String,
}

/// scope 校验：Agent 调用方只能操作自己启动的进程
fn check_scope(ctx: &RequestContext, entry: &ProcessEntry) -> Result<()> {
    if let Some(agent_id) = ctx.agent_id()
        && entry.agent_id.as_deref() != Some(agent_id.as_str())
    {
        return Err(common::error::Error::forbidden(format!(
            "agent {} cannot manage process {} owned by {:?}",
            agent_id, entry.pid, entry.agent_id
        )));
    }
    Ok(())
}

fn not_found(pid: u32) -> common::error::Error {
    common::error::Error::not_found(format!("process {} not found in registry", pid))
}

impl super::ProcessManager for SystemDomainImpl {
    fn get_process(&self, ctx: RequestContext, pid: u32) -> Result<ProcessEntry> {
        let entry = process::registry()
            .refresh(pid)
            .ok_or_else(|| not_found(pid))?;
        check_scope(&ctx, &entry)?;
        Ok(entry)
    }

    fn list_processes(&self, ctx: RequestContext) -> Result<Vec<ProcessEntry>> {
        let entries = process::registry().list();
        // Agent 调用方仅可见自己启动的进程；人类用户/管理面可见全部
        Ok(match ctx.agent_id() {
            Some(agent_id) => entries
                .into_iter()
                .filter(|e| e.agent_id.as_deref() == Some(agent_id.as_str()))
                .collect(),
            None => entries,
        })
    }

    fn kill_process(&self, ctx: RequestContext, pid: u32) -> Result<bool> {
        let entry = process::registry().get(pid).ok_or_else(|| not_found(pid))?;
        check_scope(&ctx, &entry)?;

        if matches!(entry.status, process::ProcessStatus::Exited) {
            return Ok(false);
        }
        process::terminate(pid)?;
        process::registry().mark_exited(pid, None);
        log_info!(
            "process {} terminated by {:?}",
            pid,
            ctx.caller_id_or_system()
        );
        Ok(true)
    }

    fn process_status(
        &self,
        ctx: RequestContext,
        pid: u32,
        tail_lines: Option<usize>,
    ) -> Result<ProcessStatusDetail> {
        let entry = process::registry()
            .refresh(pid)
            .ok_or_else(|| not_found(pid))?;
        check_scope(&ctx, &entry)?;

        let tail_lines = tail_lines.unwrap_or(20).min(500);
        let log_tail = process::tail_log(&entry.log_path, tail_lines);
        Ok(ProcessStatusDetail { entry, log_tail })
    }
}

#[cfg(test)]
mod tests {
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
}
