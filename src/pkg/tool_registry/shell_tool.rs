//! 声明式 Shell 工具运行时
//!
//! 与 HTTP 工具同构的「数据库注册 + 配置驱动」模式：`ToolPo.config` 存储
//! JSON 序列化的 [`ShellToolConfig`]，registry 把持久化元数据构造成可执行的
//! [`ShellCoreTool`]。给模型不熟的命令用——模型对着结构化参数 schema 传参，
//! 而不是猜 CLI flags。
//!
//! # 安全边界
//!
//! - **argv 逐项传递，不经 `sh -c`**：参数填充模板后作为独立 argv 项直接
//!   exec，空白/元字符无解释歧义，命令注入结构性不可能
//! - **program 固定**：可执行文件由配置期声明，模型只能在参数 schema 约束
//!   下填充 `{{args.x}}` 占位符，不能指定要跑什么程序
//! - **复用 shell 拦截层**：执行前将渲染后的命令行过 `shell_policy::evaluate`
//!   （scope/危险命令/审计），与 shell_exec 同一套策略管线
//! - **复用 `pkg/process::exec`**：wait_with_output 防管道死锁、超时必终止

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::process::{self, ExecOptions};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::http::{render_string_template, validate_args_schema};
use crate::pkg::tool_registry::shell_policy::{self, ShellPolicyInput};
use anyhow::anyhow;
use async_trait::async_trait;
use common::err;
use common::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

/// 默认执行超时：60s（对齐 `pkg/process::DEFAULT_EXEC_TIMEOUT`）
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
/// 硬上限执行超时：10 分钟（对齐 `pkg/process::MAX_EXEC_TIMEOUT`）
pub const MAX_TIMEOUT_MS: u64 = 600_000;
/// stdout/stderr 截断上限（字符）：保护上下文不被海量输出淹没
const OUTPUT_TRUNCATE_CHARS: usize = 100_000;

/// 全局配置未初始化时的 base_root 兜底（测试环境）
pub fn test_fallback_base_root() -> PathBuf {
    std::env::temp_dir().join("ai_orz_test_base")
}

/// 声明式 Shell 工具配置（存储于 `ToolPo.config`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellToolConfig {
    /// 可执行文件（固定 program，模型不可指定；纯名称走 PATH 解析）
    pub program: String,
    /// argv 模板：支持 `{{args.x}}` 占位符，渲染后逐项传递（不经 shell 解释）
    #[serde(default)]
    pub args_template: Vec<String>,
    /// 工作目录（绝对路径；None = 继承父进程）
    pub working_dir: Option<String>,
    /// 执行超时毫秒（默认 60s，硬上限 10 分钟）
    pub timeout_ms: Option<u64>,
}

/// 可执行的声明式 Shell 工具
#[derive(Debug, Clone)]
pub struct ShellCoreTool {
    po: ToolPo,
    config: ShellToolConfig,
}

impl ShellCoreTool {
    /// Build a shell core tool from a persistent ToolPo.
    pub fn from_po(po: ToolPo) -> Result<Self> {
        let config: ShellToolConfig = serde_json::from_value(po.config.clone())
            .map_err(|e| anyhow!("invalid shell tool config for {}: {}", po.id, e))?;
        validate_config(&config)
            .map_err(|e| anyhow!("invalid shell tool config for {}: {}", po.id, e))?;
        Ok(Self { po, config })
    }

    pub fn config(&self) -> &ShellToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for ShellCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        execute_shell_call(&ctx, &self.config, self.po.parameters_schema.as_ref(), args)
            .await
            .map_err(|e| err!(ToolExecutionFailed, e.to_string()))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Create an executable shell tool from ToolPo.
pub fn create_tool(po: ToolPo) -> Result<Box<dyn CoreTool>> {
    Ok(Box::new(ShellCoreTool::from_po(po)?))
}

/// Validate a shell ToolPo without constructing the executable runtime.
///
/// Management APIs call this before persisting shell tool configs so invalid
/// definitions are rejected at configuration time, not only at runtime.
pub fn validate_tool_po_config(po: &ToolPo) -> Result<()> {
    let config: ShellToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid shell tool config for {}: {}", po.id, e))?;
    validate_config(&config)
}

async fn execute_shell_call(
    ctx: &RequestContext,
    config: &ShellToolConfig,
    parameters_schema: Option<&Value>,
    args: Value,
) -> Result<Value> {
    validate_args_schema(parameters_schema, &args)?;

    // 模板渲染：占位符填充进 argv 项（逐项传递，无 shell 解释）
    let mut argv = Vec::with_capacity(config.args_template.len());
    for template in &config.args_template {
        argv.push(render_string_template(template, &args)?);
    }

    // 复用 shell 拦截层：scope/危险命令/审计与 shell_exec 同一套策略管线
    // （try_get 兜底：测试环境可能未初始化全局配置）
    let base_root = crate::config::try_get()
        .map(|cfg| cfg.base_data_path())
        .unwrap_or_else(|| std::env::temp_dir().join(".ai_orz"));
    let working_dir = config
        .working_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&base_root));
    let verdict = shell_policy::evaluate(ShellPolicyInput {
        command: &format!("{} {}", config.program, argv.join(" ")),
        working_dir: &working_dir,
        base_root: &base_root.to_string_lossy(),
        additional_allowed_paths: &[],
        user_id: ctx.user_id.as_deref(),
        agent_id: ctx.agent_id.as_deref(),
    });
    if let Some(blocking) = verdict.blocking {
        return Ok(json!({
            "success": false,
            "require_confirmation": true,
            "error": blocking.reason(),
            "message": blocking.reason(),
        }));
    }
    for (rule_id, reason) in verdict.audits {
        crate::log_info!(
            ctx,
            "shell_tool",
            tool_id = ctx.agent_id.as_deref().unwrap_or("-"),
            policy = rule_id,
            reason = reason,
            "声明式 shell 工具执行审计"
        );
    }

    let timeout_ms = config.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    // working_dir 不存在时先创建（对齐 shell_exec 行为；current_dir 缺失时
    // spawn 会报 ENOENT 且被误归因到 binary not found）
    if let Err(e) = tokio::fs::create_dir_all(&working_dir).await {
        return Err(anyhow!("create working_dir '{}' failed: {e}", working_dir.display()).into());
    }
    let output = process::exec(
        &ExecOptions::new(&config.program, argv)
            .timeout(Duration::from_millis(timeout_ms))
            .current_dir(&working_dir),
    )
    .await?;

    Ok(json!({
        "success": output.success,
        "exit_code": output.exit_code,
        "stdout": truncate_lossy(&output.stdout),
        "stderr": truncate_lossy(&output.stderr),
        "timed_out": output.timed_out,
    }))
}

fn truncate_lossy(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= OUTPUT_TRUNCATE_CHARS {
        return text.into_owned();
    }
    let truncated: String = text.chars().take(OUTPUT_TRUNCATE_CHARS).collect();
    format!("{truncated}\n...[truncated]")
}

/// 构造期 + 调用前共用的配置校验
pub fn validate_config(config: &ShellToolConfig) -> Result<()> {
    let program = config.program.trim();
    if program.is_empty() {
        return Err(anyhow!("shell tool program is required").into());
    }
    if program.chars().any(|c| c.is_whitespace()) {
        return Err(anyhow!("shell tool program must not contain whitespace").into());
    }
    for template in &config.args_template {
        // 复用 HTTP 模板占位符校验（{{args.x}} 语法单一事实源）
        crate::pkg::tool_registry::http::validate_supported_placeholders(template)?;
    }
    if let Some(dir) = &config.working_dir {
        let path = PathBuf::from(dir);
        if !path.is_absolute() {
            return Err(anyhow!("shell tool working_dir must be an absolute path").into());
        }
    }
    let timeout_ms = config.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout_ms == 0 || timeout_ms > MAX_TIMEOUT_MS {
        return Err(anyhow!(
            "invalid shell timeout_ms: {} (must be 1..={})",
            timeout_ms,
            MAX_TIMEOUT_MS
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "shell_tool_tests.rs"]
mod tests;
