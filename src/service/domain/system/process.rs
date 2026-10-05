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
#[path = "process_tests.rs"]
mod tests;
