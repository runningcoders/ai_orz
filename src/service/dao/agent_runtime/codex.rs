//! Codex / CLI Agent Runtime DAO
//!
//! 通过子进程 stdin/stdout 方式调用 CLI 类型的外部 Agent（如 Codex、Claude Code 等）。
//! 将 prompt 写入子进程 stdin，读取 stdout 作为执行结果。

use async_trait::async_trait;
use common::error::{Result, err};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::AgentRuntimeDao;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;

/// CLI Agent 执行配置
#[derive(Debug, Clone)]
pub struct CliRuntimeConfig {
    /// 可执行命令
    pub command: String,
    /// 命令参数
    pub args: Vec<String>,
    /// 工作目录
    pub work_dir: String,
    /// 额外环境变量
    pub env: Vec<(String, String)>,
    /// 超时时间（秒）
    pub timeout_secs: u64,
    /// prompt 模板（可选），用于包装 prompt
    /// 例如："你是一个助手，请回答以下问题：\n{prompt}"
    pub prompt_template: Option<String>,
}

/// Codex / CLI Agent Runtime DAO
#[derive(Debug, Clone)]
pub struct CodexRuntimeDao {
    config: CliRuntimeConfig,
}

impl CodexRuntimeDao {
    pub fn new(config: CliRuntimeConfig) -> Self {
        Self { config }
    }

    /// 应用 prompt 模板
    fn apply_prompt_template(&self, prompt: &str) -> String {
        match &self.config.prompt_template {
            Some(template) => template.replace("{prompt}", prompt),
            None => prompt.to_string(),
        }
    }
}

#[async_trait]
impl AgentRuntimeDao for CodexRuntimeDao {
    async fn invoke(&self, _ctx: RequestContext, agent: &AgentPo, prompt: &str) -> Result<String> {
        execute_cli(
            &agent.id,
            &self.config.command,
            &self.config.args,
            &self.config.work_dir,
            &self.config.env,
            self.config.timeout_secs,
            &self.apply_prompt_template(prompt),
        )
        .await
    }
}

/// 执行 CLI 命令，通过 stdin 传入 prompt，读取 stdout 作为结果
pub async fn execute_cli(
    agent_id: &str,
    command: &str,
    args: &[String],
    work_dir: &str,
    env: &[(String, String)],
    timeout_secs: u64,
    prompt: &str,
) -> Result<String> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in env {
        cmd.env(key, value);
    }

    let mut child = cmd.spawn().map_err(|e| {
        err!(
            Internal,
            "Agent {}: failed to spawn CLI command '{}': {}",
            agent_id,
            command,
            e
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
            err!(
                Internal,
                "Agent {}: failed to write to stdin: {}",
                agent_id,
                e
            )
        })?;
        drop(stdin);
    }

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    let status_result = tokio::time::timeout(timeout, child.wait()).await;

    match status_result {
        Ok(Ok(status)) => {
            let mut output = Vec::new();
            let mut stderr_output = Vec::new();
            if let Some(mut out) = stdout.take() {
                use tokio::io::AsyncReadExt;
                let _ = out.read_to_end(&mut output).await;
            }
            if let Some(mut err) = stderr.take() {
                use tokio::io::AsyncReadExt;
                let _ = err.read_to_end(&mut stderr_output).await;
            }

            if !status.success() {
                let output_str = String::from_utf8_lossy(&output);
                let stderr_str = String::from_utf8_lossy(&stderr_output);
                return Err(err!(
                    Internal,
                    "Agent {}: CLI command exited with status {:?}: stdout={}, stderr={}",
                    agent_id,
                    status.code(),
                    output_str.trim(),
                    stderr_str.trim()
                ));
            }
            let output_str = String::from_utf8_lossy(&output);
            Ok(output_str.trim().to_string())
        }
        Ok(Err(e)) => Err(err!(
            Internal,
            "Agent {}: CLI command execution failed: {}",
            agent_id,
            e
        )),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(err!(
                Internal,
                "Agent {}: CLI command timed out after {} seconds",
                agent_id,
                timeout_secs
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_cli_echo() {
        let result = execute_cli(
            "test-agent",
            "echo",
            &["hello world".to_string()],
            "/tmp",
            &[],
            10,
            "unused",
        )
        .await;

        assert!(result.is_ok(), "echo command failed: {:?}", result.err());
        assert_eq!(result.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_execute_cli_cat_stdin() {
        let result = execute_cli(
            "test-agent",
            "cat",
            &[],
            "/tmp",
            &[],
            10,
            "hello from stdin",
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello from stdin");
    }

    #[tokio::test]
    async fn test_execute_cli_timeout() {
        let result = execute_cli(
            "test-agent",
            "sleep",
            &["5".to_string()],
            "/tmp",
            &[],
            1,
            "",
        )
        .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("timed out"));
    }

    #[tokio::test]
    async fn test_execute_cli_non_zero_exit() {
        let result = execute_cli(
            "test-agent",
            "sh",
            &["-c".to_string(), "echo error msg >&2; exit 1".to_string()],
            "/tmp",
            &[],
            10,
            "",
        )
        .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("error msg"));
    }

    #[test]
    fn test_prompt_template() {
        let dao = CodexRuntimeDao::new(CliRuntimeConfig {
            command: "cat".to_string(),
            args: vec![],
            work_dir: "/tmp".to_string(),
            env: vec![],
            timeout_secs: 10,
            prompt_template: Some(
                "System: You are helpful.\nUser: {prompt}\nAssistant:".to_string(),
            ),
        });

        let result = dao.apply_prompt_template("Hello");
        assert_eq!(result, "System: You are helpful.\nUser: Hello\nAssistant:");
    }

    #[test]
    fn test_no_prompt_template() {
        let dao = CodexRuntimeDao::new(CliRuntimeConfig {
            command: "cat".to_string(),
            args: vec![],
            work_dir: "/tmp".to_string(),
            env: vec![],
            timeout_secs: 10,
            prompt_template: None,
        });

        let result = dao.apply_prompt_template("Hello");
        assert_eq!(result, "Hello");
    }
}
