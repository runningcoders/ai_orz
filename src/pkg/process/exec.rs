//! 子进程执行原语（生产端）
//!
//! 与本模块的注册中心（[`super`]，管理端）互补，两层职责：
//! - **exec**：「怎么跑」——spawn、超时终止、输出捕获，面向短命 CLI 调用
//!   （gh/lark/browser/codex 等）。不进注册中心：毫秒级调用注册进去是噪音，
//!   且注册中心条目带 agent_id/call_id，是给 shell_kill 权限边界用的。
//! - **registry**：「跑起来之后」——注册、探活、终止、审计，面向 Agent
//!   可管理的长生命周期进程（如 shell_exec 后台模式）。
//!
//! # 硬约束
//!
//! - **输出捕获恒用 `wait_with_output()`**（并发读管道）：先 `wait()` 后读
//!   stdout 的写法在子进程输出超过管道缓冲区（~64KB）时会双向阻塞直到超时，
//!   输出全丢——此类死锁在本原语下结构性不可能发生
//! - **超时必终止**：超时丢弃 future 时由 `kill_on_drop` 终止子进程，
//!   tokio 后台 orphan reaper 负责回收，无僵尸进程
//! - **stdin 写入 best-effort**：命令未读 stdin 即退出（Broken pipe）是合法
//!   行为——结果由 stdout 决定，不视为失败

use common::error::{Error, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// 默认执行超时：60s
pub const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(60);
/// 硬上限执行超时：10 分钟
pub const MAX_EXEC_TIMEOUT: Duration = Duration::from_secs(600);

/// 子进程执行选项
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// 可执行文件路径（不经 shell 解析，直接 exec）
    pub program: String,
    /// 参数列表（逐项传递，无空白切分歧义）
    pub args: Vec<String>,
    /// 工作目录（None = 继承父进程）
    pub current_dir: Option<PathBuf>,
    /// 附加环境变量（在父进程环境上叠加）
    pub env: Vec<(String, String)>,
    /// 写入 stdin 后立即关闭（None = 不接管 stdin，子进程 stdin 为 null）
    pub stdin: Option<Vec<u8>>,
    /// 执行超时；`None` 或 0 → [`DEFAULT_EXEC_TIMEOUT`]，超 [`MAX_EXEC_TIMEOUT`] 截断
    pub timeout: Option<Duration>,
}

impl ExecOptions {
    /// 创建执行选项（必填 program + args）
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            current_dir: None,
            env: Vec::new(),
            stdin: None,
            timeout: None,
        }
    }

    /// 设置工作目录
    pub fn current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// 追加环境变量
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// 追加一组环境变量
    pub fn envs(mut self, vars: Vec<(String, String)>) -> Self {
        self.env.extend(vars);
        self
    }

    /// 设置 stdin 输入（写入后关闭）
    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    /// 设置执行超时（零值按未指定处理）
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 有效超时：未指定或 0 → [`DEFAULT_EXEC_TIMEOUT`]；超 [`MAX_EXEC_TIMEOUT`] → 截断
    pub fn effective_timeout(&self) -> Duration {
        match self.timeout {
            Some(timeout) if timeout.as_nanos() > 0 => timeout.min(MAX_EXEC_TIMEOUT),
            _ => DEFAULT_EXEC_TIMEOUT,
        }
    }
}

/// 子进程执行结果
#[derive(Debug, Clone)]
pub struct ExecOutput {
    /// 子进程 pid（spawn 失败时不产生结果，恒有值）
    pub pid: u32,
    /// 是否正常退出（exit code 0）
    pub success: bool,
    /// 退出码（被信号杀死时为 None）
    pub exit_code: Option<i32>,
    /// stdout 原始字节
    pub stdout: Vec<u8>,
    /// stderr 原始字节
    pub stderr: Vec<u8>,
    /// 是否因超时被终止（终止时 stdout/stderr 为空——管道内容随 kill 丢弃）
    pub timed_out: bool,
}

/// 执行子进程并捕获输出（超时终止、并发读管道）
///
/// # 契约
///
/// - stdout/stderr 恒为 piped 并发读取（无管道死锁）
/// - 超时：kill 子进程并返回 `timed_out = true`（不报错，由调用方决定语义）
/// - spawn 失败才返回 `Err`（如二进制不存在）
pub async fn exec(options: &ExecOptions) -> Result<ExecOutput> {
    let mut command = Command::new(&options.program);
    command
        .args(&options.args)
        .stdin(if options.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 超时丢弃 future 时同步终止子进程（tokio 后台回收，无僵尸）
        .kill_on_drop(true);

    if let Some(dir) = &options.current_dir {
        command.current_dir(dir);
    }
    for (key, value) in &options.env {
        command.env(key, value);
    }

    let mut child = command.spawn().map_err(|e| {
        // spawn 失败分类保留错误码：NotFound/PermissionDenied 供调用方给出
        // 安装引导/权限提示（如 browser 工具），其余归 Internal
        match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::not_found(format!("binary '{}' not found: {e}", options.program))
            }
            std::io::ErrorKind::PermissionDenied => {
                Error::forbidden(format!("binary '{}' not executable: {e}", options.program))
            }
            _ => Error::internal(format!("spawn '{}' failed: {e}", options.program)),
        }
    })?;
    let pid = child.id().unwrap_or_default();

    // stdin 注入（best-effort）：Broken pipe = 命令未读 stdin 即退出，合法
    if let Some(input) = &options.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        if let Err(e) = stdin.write_all(input).await
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            sys_warn!("exec stdin write to '{}' failed: {}", options.program, e);
        }
        drop(stdin); // 关闭写端，子进程读到 EOF
    }

    let timeout = options.effective_timeout();
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(ExecOutput {
            pid,
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
            timed_out: false,
        }),
        Ok(Err(e)) => Err(Error::internal(format!(
            "execute '{}' failed: {e}",
            options.program
        ))),
        // 超时：future 连同 child 一起被丢弃，kill_on_drop 已终止
        Err(_) => Ok(ExecOutput {
            pid,
            success: false,
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            timed_out: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_captures_stdout_and_exit_code() {
        let out = exec(&ExecOptions::new("echo", vec!["hello".into()]))
            .await
            .expect("echo should succeed");
        assert!(out.success);
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
        assert!(!out.timed_out);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_captures_stderr_on_failure() {
        let out = exec(&ExecOptions::new(
            "sh",
            vec!["-c".into(), "echo err >&2; exit 3".into()],
        ))
        .await
        .expect("sh should spawn");
        assert!(!out.success);
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_times_out_and_kills() {
        let out =
            exec(&ExecOptions::new("sleep", vec!["30".into()]).timeout(Duration::from_millis(150)))
                .await
                .expect("sleep spawn ok");
        assert!(out.timed_out);
        assert!(!out.success);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_large_output_does_not_deadlock() {
        // 2MB 输出远超管道缓冲区：若先 wait 后读会永久阻塞直到超时
        let out = exec(&ExecOptions::new(
            "sh",
            vec![
                "-c".into(),
                "head -c 2097152 /dev/zero | tr '\\0' 'x'".into(),
            ],
        ))
        .await
        .expect("large output should complete");
        assert!(out.success);
        assert_eq!(out.stdout.len(), 2 * 1024 * 1024);
        assert!(!out.timed_out);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_feeds_stdin_and_survives_broken_pipe() {
        // 读 stdin 的命令：echo 收到注入内容
        let out = exec(&ExecOptions::new("cat", vec![]).stdin("piped-data"))
            .await
            .expect("cat should succeed");
        assert!(out.success);
        assert_eq!(out.stdout, b"piped-data");

        // 不读 stdin 即退出的命令：Broken pipe 属合法行为，不报错
        let out = exec(&ExecOptions::new("echo", vec!["done".into()]).stdin("ignored"))
            .await
            .expect("echo ignoring stdin should still succeed");
        assert!(out.success);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_applies_env_and_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let out = exec(
            &ExecOptions::new("sh", vec!["-c".into(), "pwd; echo $EXEC_TEST_VAR".into()])
                .current_dir(dir.path())
                .env("EXEC_TEST_VAR", "exec-env-value"),
        )
        .await
        .expect("sh should succeed");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("exec-env-value"));
        assert!(text.contains(dir.path().to_str().unwrap()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_spawn_failure_is_err() {
        let result = exec(&ExecOptions::new(
            "/nonexistent/binary/for/exec-test",
            vec![],
        ))
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn timeout_defaults_and_clamps() {
        assert_eq!(
            ExecOptions::new("x", vec![]).effective_timeout(),
            DEFAULT_EXEC_TIMEOUT
        );
        assert_eq!(
            ExecOptions::new("x", vec![])
                .timeout(Duration::ZERO)
                .effective_timeout(),
            DEFAULT_EXEC_TIMEOUT
        );
        assert_eq!(
            ExecOptions::new("x", vec![])
                .timeout(MAX_EXEC_TIMEOUT + Duration::from_secs(1))
                .effective_timeout(),
            MAX_EXEC_TIMEOUT
        );
    }
}
