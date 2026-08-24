//! Builtin shell_exec tool implementation
//!
//! Execute shell commands asynchronously, support short commands sync wait,
//! long commands background running with output logging.

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::paths;
use crate::pkg::process::{self, ProcessEntry, ProcessStatus};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_security::fs::{
    crosses_agent_workspace, crosses_user_boundary,
};
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
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
    /// Allowed environment variable names (whitelist).
    /// Only these environment variables from the parent process will be passed to the child.
    pub allowed_env: Option<Vec<String>>,
}

impl Default for ShellExecConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: None,
            default_max_output_size_bytes: None,
            additional_allowed_paths: None,
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
            name: "shell_exec".to_string(),
            description: concat!(
                "Execute shell commands in a sandboxed environment. ",
                "Supports both short synchronous execution and long asynchronous background processes. ",
                "Output larger than the configured limit is stored as a log attachment, only summary returned. ",
                "**Default working directory**: when omitted, commands run in the calling agent's workspace ",
                "(users/{user_id}/agents/{agent_id}/work), with HOME set to the user's isolated home directory ",
                "(so git/gh and other CLIs reuse the user's configuration). ",
                "**Security**: Working directory is restricted to configured allowed paths; another user's tree ",
                "or another agent's workspace requires explicit user confirmation; sensitive environment ",
                "variables are filtered out."
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

    /// Validate resolved working directory is within allowed scope
    /// (base data path or configured additional allowed paths).
    fn validate_working_dir(&self, resolved: &std::path::Path) -> bool {
        let base_path = get().base_data_path();
        let base_path = std::path::Path::new(&base_path);
        if resolved.starts_with(base_path) {
            return true;
        }
        self.config
            .additional_allowed_paths()
            .iter()
            .any(|allowed| resolved.starts_with(std::path::Path::new(allowed)))
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
pub fn filter_inherited_environment(allowed: &[String]) -> HashMap<String, String> {
    let sensitive_vars: &[&str] = &[
        "home",
        "user",
        "username",
        "password",
        "token",
        "secret",
        "api_key",
        "aws_access_key_id",
        "aws_secret_access_key",
        "google_application_credentials",
        "ssh_auth_sock",
        "git_config",
        "git_ssh",
    ];

    std::env::vars()
        .filter(|(key, _)| {
            // Check if key is in allowed list
            if !allowed.contains(&key.to_string()) {
                return false;
            }
            // Filter out sensitive variables even if allowed
            let key_lower = key.to_lowercase();
            !sensitive_vars.iter().any(|s| key_lower.contains(s))
        })
        .collect()
}

/// Merge extra environment variables into base environment.
pub fn merge_extra_environment(
    mut base: HashMap<String, String>,
    extra: &Value,
) -> HashMap<String, String> {
    if let Some(obj) = extra.as_object() {
        for (key, value) in obj {
            if let Some(val_str) = value.as_str() {
                base.insert(key.clone(), val_str.to_string());
            }
        }
    }
    base
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

        // Validate scope (base data path / additional allowed paths)
        if !self.validate_working_dir(&working_dir) {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!("Working directory '{}' is not in allowed paths", working_dir.display()),
                "require_confirmation": true
            }));
        }

        // Workspace identity boundary: another user's tree / another agent's
        // workspace requires explicit user confirmation.
        let base_root = get().base_data_path();
        if crosses_user_boundary(&base_root, &working_dir, ctx.user_id.as_deref())
            || crosses_agent_workspace(&base_root, &working_dir, ctx.agent_id.as_deref())
        {
            return Ok(serde_json::json!({
                "success": false,
                "require_confirmation": true,
                "message": format!(
                    "Working directory '{}' belongs to another user/agent workspace. \
                    You MUST STOP and ask the user for explicit confirmation before using it.",
                    working_dir.display()
                )
            }));
        }

        if !working_dir.exists() {
            create_dir_all(&working_dir).await?;
        }

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

        // Prepare environment
        let mut env = filter_inherited_environment(self.config.allowed_env());
        if let Some(extra_env) = &params.env {
            env = merge_extra_environment(env, &serde_json::to_value(extra_env)?);
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
        // 用户身份存在时，HOME 指向用户隔离 HOME（见 paths::user_home），
        // 让 git/gh 等子命令复用该用户的 CLI 配置与凭证
        if let Some(uid) = ctx.user_id.as_deref() {
            command.env("HOME", paths::user_home(&base_root, uid));
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
