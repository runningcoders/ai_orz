//! Builtin shell_exec tool implementation
//!
//! Execute shell commands asynchronously, support short commands sync wait,
//! long commands background running with output logging.

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::git_workspace;
use crate::pkg::paths;
use crate::pkg::process::{self, ProcessEntry, ProcessStatus};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::shell_env;
use crate::pkg::tool_registry::shell_policy::{self, ShellPolicyInput};
use anyhow::anyhow;
use common::config::HomeMode;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::fs::{OpenOptions, create_dir_all};
use tokio::process::Command;

/// ShellExec tool configuration stored in `ToolPo.config`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
/// Configuration for shell_exec tool.
pub struct ShellExecConfig {
    /// Default timeout in milliseconds.
    pub default_timeout_ms: Option<u64>,
    /// Default maximum output size in bytes.
    pub default_max_output_size_bytes: Option<u64>,
    /// Additional allowed paths for execution (beyond base data path).
    pub additional_allowed_paths: Option<Vec<String>>,
    /// 追加到子进程 PATH 尾部的目录（None = 内置默认，见 `common::config::ShellConfig`）
    ///
    /// 支持 `~` 前缀与单段 `*` 通配（如 `~/.nvm/versions/node/*/bin`）；
    /// 仅追加「存在且尚未出现在 PATH 中」的目录，不覆盖既有解析顺序。
    pub path_additions: Option<Vec<String>>,
    /// 子进程 HOME 策略（None = 内置默认 `isolated`）
    pub home_mode: Option<HomeMode>,
    /// 隔离 HOME 下把工具链根目录指回真实 HOME 的工具链名单（None = 不注入）
    ///
    /// 仅 `home_mode = isolated` 时生效：git/gh 走隔离身份的同时，名单内工具链
    /// 经官方环境变量（`CARGO_HOME` / `NVM_DIR` 等，见
    /// `common::models::tool::SHELL_TOOLCHAIN_HOME_VARS`）读取真实 HOME 配置。
    /// 路径不存在或未知名忽略；`params.env` 显式传的同名变量优先。
    pub toolchain_envs: Option<Vec<String>>,
    /// 显式注入/覆盖的环境变量名（取值来自服务进程环境）
    ///
    /// **这不是安全边界**：子进程默认继承服务进程**全部**环境变量——兼容优先，
    /// 因为 `TMPDIR` / `LANG` / `DYLD_*` / `XDG_*` 等被剔除会直接搞挂大量 CLI，
    /// 而命令本身已由 `shell_policy` 拦截 + Manual 批准兜底，环境变量白名单的
    /// 边际收益不值这个兼容性代价。这里声明的只是「额外显式带上」的变量
    /// （敏感子串仍会剔除），默认 `PATH`（也是 PATH 补全的锚点）。
    pub allowed_env: Option<Vec<String>>,
}

impl Default for ShellExecConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: None,
            default_max_output_size_bytes: None,
            additional_allowed_paths: None,
            path_additions: None,
            home_mode: None,
            toolchain_envs: None,
            allowed_env: Some(vec!["PATH".to_string()]),
        }
    }
}

impl ShellExecConfig {
    /// Get default timeout in milliseconds.
    pub fn default_timeout_ms(&self) -> u64 {
        self.default_timeout_ms.unwrap_or(300_000)
    }

    /// Get default max output size in bytes.
    pub fn default_max_output_size_bytes(&self) -> u64 {
        self.default_max_output_size_bytes
            .unwrap_or(10 * 1024 * 1024)
    }

    /// Get additional allowed paths.
    pub fn additional_allowed_paths(&self) -> &[String] {
        self.additional_allowed_paths.as_deref().unwrap_or(&[])
    }

    /// Get allowed environment variable names.
    pub fn allowed_env(&self) -> &[String] {
        self.allowed_env.as_deref().unwrap_or(&[])
    }

    /// Get PATH 补全目录（未配置时回退内置默认）
    pub fn path_additions(&self) -> Vec<String> {
        self.path_additions
            .clone()
            .unwrap_or_else(|| common::config::ShellConfig::default().path_additions)
    }

    /// Get HOME 策略（未配置时回退内置默认 `isolated`）
    pub fn home_mode(&self) -> HomeMode {
        self.home_mode.unwrap_or_default()
    }

    /// Get 工具链根目录指回名单（未配置时为空 = 不注入）
    pub fn toolchain_envs(&self) -> &[String] {
        self.toolchain_envs.as_deref().unwrap_or(&[])
    }
}

/// `shell_exec` tool parameters.
#[derive(Debug, Deserialize)]
pub struct ShellExecParams {
    /// Shell command to execute.
    pub command: String,
    /// Working directory for command execution.
    /// If not specified, uses default from config or base_data_path.
    pub working_dir: Option<String>,
    /// Timeout in milliseconds (overrides default).
    pub timeout_ms: Option<u64>,
    /// Maximum output size in bytes (overrides default).
    pub max_output_size_bytes: Option<u64>,
    /// Run in background (don't wait for completion).
    /// For long-running processes. PID will be stored in tool call metadata.
    pub background: Option<bool>,
    /// Action on sync timeout: "detach" (default, hand process back to caller,
    /// inspect via shell_status / stop via shell_kill) or "kill" (terminate immediately).
    pub timeout_action: Option<String>,
    /// Additional environment variables to set for the command.
    pub env: Option<HashMap<String, String>>,
}

/// Factory for creating shell_exec builtin tool.
#[derive(Debug, Clone, Default)]
pub struct ShellExecToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for ShellExecToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "shell_exec".to_string(),
            name: "Execute Shell Command".to_string(),
            description: concat!(
                "Execute shell commands in a sandboxed environment. ",
                "Supports both short synchronous execution and long asynchronous background processes. ",
                "Output larger than the configured limit is stored as a log attachment, only summary returned. ",
                "**Default working directory**: when omitted, commands run in the calling agent's workspace ",
                "(users/{user_id}/agents/{agent_id}/work), with HOME set to the user's isolated home directory ",
                "(so git/gh and other CLIs reuse your own configuration). ",
                "Working directories outside the allowed scope are never executed — the tool returns require_confirmation; for another user's or agent's workspace stop and ask the user for explicit confirmation first. ",
                "Each call requires user approval (manual control mode). ",
                "The child process inherits the service process environment (the shell is non-interactive, so rc files are NOT sourced). ",
                "If a command is reported as not found, it is outside the service PATH: use an absolute path, or pass PATH explicitly via the env parameter."
            ).to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Manual,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute. Uses system shell (/bin/sh on Unix, cmd.exe on Windows)."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional: working directory for execution, relative to the base data root. Default: the calling agent's workspace (users/{user_id}/agents/{agent_id}/work)."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: timeout in milliseconds. Default: 300000 (5 minutes)."
                    },
                    "max_output_size_bytes": {
                        "type": "integer",
                        "description": "Optional: maximum output size before truncation. Default: 10485760 (10MB)."
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Optional: run in background without waiting for completion. Default: false."
                    },
                    "timeout_action": {
                        "type": "string",
                        "enum": ["detach", "kill"],
                        "description": "Optional: what to do on sync timeout. 'detach' (default) keeps the process running and returns its pid for later shell_status/shell_kill management; 'kill' terminates it immediately."
                    },
                    "env": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Optional: additional environment variables to set."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            config: serde_json::json!(ShellExecConfig::default()),
            tags: serde_json::to_string(&vec!["shell".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(ShellExecCoreTool::new(po))
    }
}

/// Core implementation of shell_exec tool.
#[derive(Debug, Clone)]
pub struct ShellExecCoreTool {
    po: ToolPo,
    config: ShellExecConfig,
}

impl ShellExecCoreTool {
    fn new(po: ToolPo) -> Self {
        let config = if po.config.is_null() {
            ShellExecConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self { po, config }
    }

    /// Resolve absolute working directory path.
    ///
    /// 未指定时按调用身份选择默认工作区（见 `paths::default_workspace`）：
    /// Agent 为用户执行任务时落在 `users/{uid}/agents/{aid}/work`。
    fn resolve_working_dir(
        &self,
        ctx: &RequestContext,
        working_dir: Option<&str>,
    ) -> std::path::PathBuf {
        let base_path = get().base_data_path();
        match working_dir {
            Some(path) if std::path::Path::new(path).is_absolute() => {
                std::path::PathBuf::from(path)
            }
            Some(path) => std::path::Path::new(&base_path).join(path),
            None => paths::default_workspace(
                &base_path,
                ctx.user_id.as_deref(),
                ctx.agent_id.as_deref(),
            ),
        }
    }
}

/// Filter inherited environment variables based on allow list.
///
/// 实现已迁至 [`shell_env::filter_inherited_environment`]（三条起子进程的链路共用），
/// 此处保留转发以兼容既有调用点。
pub fn filter_inherited_environment(allowed: &[String]) -> HashMap<String, String> {
    shell_env::filter_inherited_environment(allowed)
}

/// Merge extra environment variables into base environment.
///
/// 实现已迁至 [`shell_env::merge_extra_environment`]，此处保留转发。
pub fn merge_extra_environment(
    base: HashMap<String, String>,
    extra: &Value,
) -> HashMap<String, String> {
    shell_env::merge_extra_environment(base, extra)
}

#[async_trait::async_trait]
impl CoreTool for ShellExecCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse arguments
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;

        // Resolve working directory (default: caller-identity workspace)
        let working_dir = self.resolve_working_dir(&ctx, params.working_dir.as_deref());

        // 拦截层（policy pipeline）：scope 结构规则 + 命令规则统一裁决，
        // 阻断动作（Deny/Confirm）短路返回 require_confirmation，不执行命令
        let base_root = get().base_data_path();
        let base_root_str = base_root.to_string_lossy().into_owned();
        let verdict = shell_policy::evaluate(ShellPolicyInput {
            command: &params.command,
            working_dir: &working_dir,
            base_root: &base_root_str,
            additional_allowed_paths: self.config.additional_allowed_paths(),
            user_id: ctx.user_id.as_deref(),
            agent_id: ctx.agent_id.as_deref(),
        });
        if let Some(action) = verdict.blocking {
            let reason = action.reason();
            return Ok(serde_json::json!({
                "success": false,
                "require_confirmation": true,
                "error": reason,
                "message": reason
            }));
        }

        if !working_dir.exists() {
            create_dir_all(&working_dir).await?;
        }

        // 工作区惰性 git init（产物锚点基建，best-effort 不阻断执行）
        git_workspace::ensure_workspace_repo(
            Path::new(&base_root_str),
            &working_dir,
            ctx.user_id.as_deref(),
            ctx.agent_id.as_deref(),
        )
        .await;

        // Get effective timeout and max output
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| self.config.default_timeout_ms());
        let max_output_bytes = params
            .max_output_size_bytes
            .unwrap_or_else(|| self.config.default_max_output_size_bytes());
        let timeout_action = params.timeout_action.as_deref().unwrap_or("detach");
        if timeout_action != "detach" && timeout_action != "kill" {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!(
                    "Invalid timeout_action '{}' (expected 'detach' or 'kill')",
                    timeout_action
                )
            }));
        }

        // Prepare environment（统一出口：白名单 → extra → PATH 补全 → 身份变量）
        let path_additions = self.config.path_additions();
        let mut env = shell_env::resolve(&shell_env::ShellEnvRequest {
            allowed_env: self.config.allowed_env(),
            path_additions: Some(path_additions.as_slice()),
            extra_env: params.env.as_ref(),
            task_id: ctx.task_id().map(String::as_str),
            agent_id: ctx.agent_id().map(String::as_str),
        });
        // 隔离 HOME + 声明工具链 → 工具链根目录经官方变量指回真实 HOME：
        // git/gh 走隔离身份的同时，cargo/nvm/pyenv 等仍能读真实 HOME 配置。
        // 仅在未走 env.HOME 逃生舱（params.env 未显式传 HOME）时注入；
        // or_insert 保证 params.env 显式传的同名变量优先。
        if !env.contains_key("HOME") && matches!(self.config.home_mode(), HomeMode::Isolated) {
            for (key, value) in shell_env::toolchain_env_injections(self.config.toolchain_envs()) {
                env.entry(key).or_insert(value);
            }
            // git over ssh：known_hosts 补齐（agent key 靠继承的 SSH_AUTH_SOCK，
            // 私钥文件/config 恒不注入，见 shell_env::git_ssh_command_injection 文档）
            if let Some(value) = shell_env::git_ssh_command_injection() {
                env.entry("GIT_SSH_COMMAND".to_string()).or_insert(value);
            }
        }

        // 审计：放行命中的审计规则（如 git commit = 产物锚点产生时刻）
        for (rule_id, reason) in &verdict.audits {
            log_info!(
                &ctx,
                "shell_policy_audit",
                rule = rule_id,
                reason = reason,
                "shell 命令命中审计规则"
            );
        }

        // 统一日志流式模型：日志文件名 {call_id}.log，与 ToolCallEntry 全链路关联
        // call_id 优先取 ToolCallDao::execute 注入值；直接调用（测试）回退 log_id
        let call_id = ctx
            .tool_call_id()
            .cloned()
            .unwrap_or_else(|| ctx.log_id.clone());
        // 按天分区子目录（YYYYMMDD，对齐 daily_jsonl 日期分区先例），清理单位为日期目录
        let day_dir = chrono::Local::now().format("%Y%m%d").to_string();
        let log_dir = paths::tool_logs_dir(&base_root, "shell_exec").join(&day_dir);
        if !log_dir.exists() {
            create_dir_all(&log_dir).await?;
        }
        let log_path = log_dir.join(format!("{}.log", call_id));

        // 统一执行模型：sync 与 background 都从 spawn 起把 stdout/stderr 重定向到日志文件
        let mut command = shell_command();
        command.arg(&params.command);
        command.current_dir(&working_dir);
        for (key, value) in &env {
            command.env(key, value);
        }
        // HOME 策略（ToolPo.config `home_mode`）：默认指向用户隔离 HOME（见
        // paths::user_home），让 git/gh 等子命令复用该用户的 CLI 配置与凭证；
        // `inherit` 时不注入，子进程继承服务进程 HOME（nvm / cargo / ssh 才可用）。
        // 调用方通过 env 参数显式传 HOME 时以其为准（隔离与兼容冲突时的逃生舱）
        let home = env.get("HOME").map(std::path::PathBuf::from).or_else(|| {
            shell_env::home_for(self.config.home_mode(), ctx.user_id.as_deref(), &base_root)
        });
        if let Some(home) = home {
            command.env("HOME", home);
        }
        let stdio_stdout = Stdio::from(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&log_path)
                .await?
                .into_std()
                .await,
        );
        let stdio_stderr = Stdio::from(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&log_path)
                .await?
                .into_std()
                .await,
        );
        command.stdout(stdio_stdout);
        command.stderr(stdio_stderr);

        let background = params.background.unwrap_or(false);

        // Spawn 进程
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(serde_json::json!({
                    "success": false,
                    "call_id": call_id,
                    "error": format!("Failed to spawn command: {}", e),
                    "log_path": log_path.to_string_lossy(),
                }));
            }
        };
        let pid = child.id();

        // 注册到统一进程注册中心（sync/background 均注册），供 shell_status/shell_kill 管理
        if let Some(pid) = pid {
            process::registry().register(ProcessEntry {
                pid,
                tool_id: "shell_exec".to_string(),
                call_id: call_id.clone(),
                agent_id: ctx.agent_id().cloned(),
                project_id: ctx.project_id().cloned(),
                task_id: ctx.task_id().cloned(),
                command: params.command.clone(),
                working_dir: working_dir.to_string_lossy().to_string(),
                log_path: log_path.to_string_lossy().to_string(),
                background,
                started_at: common::constants::utils::current_timestamp_ms() as u64,
                status: ProcessStatus::Running,
                exit_code: None,
                finished_at: None,
            });
        }

        if background {
            // 后台模式：立即返回，Agent 可用 shell_status 轮询 / shell_kill 终止
            let pid_str = pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            return Ok(serde_json::json!({
                "success": true,
                "background": true,
                "call_id": call_id,
                "pid": pid,
                "log_path": log_path.to_string_lossy(),
                "message": format!(
                    "Command started in background with PID {}. Use shell_status/shell_kill to inspect or stop it. Output is logged to: {}",
                    pid_str,
                    log_path.to_string_lossy()
                )
            }));
        }

        // 同步模式：带超时等待
        let timeout = std::time::Duration::from_millis(timeout_ms);
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => {
                if let Some(pid) = pid {
                    process::registry().mark_exited(pid, status.code());
                }

                // 从日志文件读取输出做摘要（受 max_output_size_bytes 截断），全量留盘
                let output = tokio::fs::read(&log_path).await.unwrap_or_default();
                let truncated = output.len() as u64 > max_output_bytes;
                let output_for_message = if truncated {
                    &output[..max_output_bytes as usize]
                } else {
                    &output
                };
                let output_str = String::from_utf8_lossy(output_for_message);
                let summary = if truncated {
                    format!(
                        "{}\n\n... [truncated] full output saved to: {}",
                        output_str,
                        log_path.to_string_lossy()
                    )
                } else {
                    output_str.to_string()
                };

                Ok(serde_json::json!({
                    "success": status.success(),
                    "call_id": call_id,
                    "pid": pid,
                    "exit_code": status.code(),
                    "truncated": truncated,
                    "full_output_bytes": output.len(),
                    "log_path": log_path.to_string_lossy(),
                    "output": summary
                }))
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                if let Some(pid) = pid {
                    process::registry().mark_exited(pid, None);
                }
                Ok(serde_json::json!({
                    "success": false,
                    "call_id": call_id,
                    "error": format!("Command execution failed: {}", e),
                    "pid": pid,
                    "log_path": log_path.to_string_lossy(),
                }))
            }
            Err(_) => {
                if timeout_action == "kill" {
                    // 显式 kill：超时立即终止
                    let _ = child.kill().await;
                    if let Some(pid) = pid {
                        process::registry().mark_exited(pid, None);
                    }
                    return Ok(serde_json::json!({
                        "success": false,
                        "status": "timeout",
                        "timeout": true,
                        "killed": true,
                        "timeout_ms": timeout_ms,
                        "call_id": call_id,
                        "pid": pid,
                        "log_path": log_path.to_string_lossy(),
                        "error": format!(
                            "Command timed out after {} ms and was killed",
                            timeout_ms
                        )
                    }));
                }
                // 默认 detach：超时不 kill，把进程交还调用方（shell_status 查询 / shell_kill 终止）
                Ok(serde_json::json!({
                    "success": false,
                    "status": "timeout",
                    "timeout": true,
                    "timeout_ms": timeout_ms,
                    "call_id": call_id,
                    "pid": pid,
                    "log_path": log_path.to_string_lossy(),
                    "message": "进程仍在运行，可用 shell_status 查询或 shell_kill 终止"
                }))
            }
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Get the appropriate shell command based on platform.
fn shell_command() -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/C");
        cmd
    } else {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c");
        cmd
    }
}
