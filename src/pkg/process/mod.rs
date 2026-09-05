//! 统一子进程基建：执行原语（exec）+ 注册中心（registry）
//!
//! 【边界】
//! - exec（生产端）：短命 CLI 调用的 spawn/超时/输出捕获，不进注册中心
//! - registry（管理端）：Agent 可管理的长生命周期进程的探活/终止/审计
//! - 第一版注册中心为内存版：服务重启后条目丢失，审计线索保留在 ToolCallEntry JSONL metadata
//! - pid 复用风险：v1 接受（OS 层 pid 可能复用），entry 携带 started_at 供人工甄别
//! - 权限边界（Agent 只能管理自己启动的进程）由 Domain 层 ProcessManager 负责

pub mod exec;

pub use exec::{DEFAULT_EXEC_TIMEOUT, ExecOptions, ExecOutput, MAX_EXEC_TIMEOUT, exec};

use common::constants::utils::current_timestamp_ms;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::Mutex;

/// 进程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProcessStatus {
    /// 运行中（spawn 后未确认退出）
    Running,
    /// 已退出（确认退出或被终止）
    Exited,
}

/// 进程条目
#[derive(Debug, Clone)]
pub struct ProcessEntry {
    /// 子进程 pid（注册中心主键）
    pub pid: u32,
    /// 启动该进程的工具 id（如 shell_exec）
    pub tool_id: String,
    /// 关联的工具调用 call_id（来自 ctx.tool_call_id，缺失时回退 log_id）
    pub call_id: String,
    /// 发起调用的 Agent ID（Agent scope 校验依据）
    pub agent_id: Option<String>,
    /// 关联 Project ID
    pub project_id: Option<String>,
    /// 关联 Task ID
    pub task_id: Option<String>,
    /// 执行的命令
    pub command: String,
    /// 工作目录
    pub working_dir: String,
    /// 输出日志文件路径
    pub log_path: String,
    /// 是否后台启动
    pub background: bool,
    /// 启动时间戳（ms）
    pub started_at: u64,
    /// 当前状态
    pub status: ProcessStatus,
    /// 退出码（同步等待结束时可得；后台进程被探活发现退出时为 None）
    pub exit_code: Option<i32>,
    /// 退出时间戳（ms）
    pub finished_at: Option<u64>,
}

/// 进程注册中心（全局单例，pid 为键）
#[derive(Default)]
pub struct ProcessRegistry {
    entries: Mutex<HashMap<u32, ProcessEntry>>,
}

static REGISTRY: OnceCell<ProcessRegistry> = OnceCell::new();

/// 获取全局进程注册中心单例
pub fn registry() -> &'static ProcessRegistry {
    REGISTRY.get_or_init(ProcessRegistry::default)
}

impl ProcessRegistry {
    /// 创建新注册中心（测试用）
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册进程条目（同 pid 覆盖旧条目）
    pub fn register(&self, entry: ProcessEntry) {
        self.entries.lock().unwrap().insert(entry.pid, entry);
    }

    /// 查询单个进程条目
    pub fn get(&self, pid: u32) -> Option<ProcessEntry> {
        self.entries.lock().unwrap().get(&pid).cloned()
    }

    /// 列出所有进程条目（按启动时间升序）
    pub fn list(&self) -> Vec<ProcessEntry> {
        let mut entries: Vec<ProcessEntry> =
            self.entries.lock().unwrap().values().cloned().collect();
        entries.sort_by_key(|e| (e.started_at, e.pid));
        entries
    }

    /// 标记进程已退出（携带退出码）
    pub fn mark_exited(&self, pid: u32, exit_code: Option<i32>) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(&pid) {
            entry.status = ProcessStatus::Exited;
            entry.exit_code = exit_code.or(entry.exit_code);
            entry.finished_at = Some(current_timestamp_ms() as u64);
        }
    }

    /// 探活并刷新状态：Running 但进程已不存在 → 标记 Exited
    ///
    /// 返回刷新后的条目（未注册返回 None）
    pub fn refresh(&self, pid: u32) -> Option<ProcessEntry> {
        {
            let mut entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get_mut(&pid)
                && entry.status == ProcessStatus::Running
                && !is_alive(pid)
            {
                entry.status = ProcessStatus::Exited;
                entry.finished_at = Some(current_timestamp_ms() as u64);
            }
        }
        self.get(pid)
    }

    /// 移除条目（测试/清理用）
    pub fn remove(&self, pid: u32) -> Option<ProcessEntry> {
        self.entries.lock().unwrap().remove(&pid)
    }
}

/// 读取日志文件尾部 n 行（文件不存在或读取失败返回空字符串）
pub fn tail_log(path: &str, lines: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

// ==================== 进程原语（平台相关） ====================

/// 探测进程是否存活
///
/// Unix 下用 `kill(pid, 0)` 信号 0 探测；非 Unix 平台返回 false（unsupported 桩）。
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return true;
    }
    // EPERM 表示进程存在但无权限发信号，同样视为存活
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub fn is_alive(_pid: u32) -> bool {
    false
}

/// 终止进程（SIGKILL）
#[cfg(unix)]
pub fn terminate(pid: u32) -> anyhow::Result<()> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    // ESRCH：进程已不存在，视为终止成功
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(anyhow::anyhow!("terminate pid {} failed: {}", pid, err))
}

#[cfg(not(unix))]
pub fn terminate(pid: u32) -> anyhow::Result<()> {
    anyhow::bail!("terminate unsupported on this platform (pid {})", pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn make_entry(pid: u32, agent_id: Option<&str>) -> ProcessEntry {
        ProcessEntry {
            pid,
            tool_id: "shell_exec".to_string(),
            call_id: format!("call-{}", pid),
            agent_id: agent_id.map(|s| s.to_string()),
            project_id: None,
            task_id: None,
            command: "sleep 10".to_string(),
            working_dir: "/tmp".to_string(),
            log_path: format!("/tmp/{}.log", pid),
            background: true,
            started_at: current_timestamp_ms() as u64,
            status: ProcessStatus::Running,
            exit_code: None,
            finished_at: None,
        }
    }

    #[test]
    fn test_register_get_list() {
        let reg = ProcessRegistry::new();
        reg.register(make_entry(1001, Some("agent-1")));
        reg.register(make_entry(1002, None));

        let entry = reg.get(1001).expect("entry should exist");
        assert_eq!(entry.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(entry.status, ProcessStatus::Running);

        let list = reg.list();
        assert_eq!(list.len(), 2);
        assert!(reg.get(9999).is_none());
    }

    #[test]
    fn test_mark_exited() {
        let reg = ProcessRegistry::new();
        reg.register(make_entry(2001, None));
        reg.mark_exited(2001, Some(0));

        let entry = reg.get(2001).unwrap();
        assert_eq!(entry.status, ProcessStatus::Exited);
        assert_eq!(entry.exit_code, Some(0));
        assert!(entry.finished_at.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn test_is_alive_and_terminate_real_process() {
        // 真实 spawn 一个 sleep 进程验证探活/终止原语
        let mut child = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();

        assert!(is_alive(pid));

        terminate(pid).expect("terminate should succeed");
        // 回收僵尸进程
        let _ = child.wait();

        assert!(!is_alive(pid));
    }

    #[cfg(unix)]
    #[test]
    fn test_refresh_marks_exited() {
        let reg = ProcessRegistry::new();
        let mut child = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();

        reg.register(make_entry(pid, Some("agent-x")));
        assert_eq!(reg.refresh(pid).unwrap().status, ProcessStatus::Running);

        terminate(pid).unwrap();
        let _ = child.wait();

        let entry = reg.refresh(pid).unwrap();
        assert_eq!(entry.status, ProcessStatus::Exited);
        assert!(entry.finished_at.is_some());
    }

    #[test]
    fn test_tail_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tail.log");
        std::fs::write(&path, "line1\nline2\nline3\nline4\n").unwrap();

        assert_eq!(tail_log(path.to_str().unwrap(), 2), "line3\nline4");
        assert_eq!(
            tail_log(path.to_str().unwrap(), 10),
            "line1\nline2\nline3\nline4"
        );
        assert_eq!(tail_log("/nonexistent/path.log", 5), "");
    }
}
