//! Builtin lark_cli tool implementation
//!
//! 通过 `lark-cli` 子进程让 Agent 操作飞书全域能力（消息/文档/日历/多维表格等）。
//!
//! # 凭证与隔离
//!
//! - 凭证不在工具入参中传递：凭据需求由工厂静态声明（同一 LarkApp 凭证
//!   app_id/app_secret/identity_mode 三字段，D4 多字段模式），domain 编排层
//!   （`resolve_tool_credentials`）据此取该用户启用 Lark 渠道的应用凭证，经
//!   `CoreTool::check` 注入实例 `credentials` 三元组字段（D17 工厂化，
//!   未注入 → 绑定引导）
//! - HOME 隔离：每次执行注入 `HOME={base_data_path}/users/{user_id}`（用户维度统一 HOME，
//!   见 `pkg::paths::user_home`），lark-cli 配置落在 `{home}/.lark-cli/`
//!   首次幂等写入该目录下的 lark-cli config（secret 走 stdin，避免进程参数泄露）
//! - 输出脱敏：返回摘要中 token/secret 类关键字按行二次过滤
//!
//! # 分层说明
//!
//! 凭据取数在 domain 编排层（D17 v1.5）：pkg 只保留纯函数与静态需求声明
//! （工厂与实例共用单点），工具实例不直连 DAL/DAO。

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_readiness;
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::{Result, err};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// lark-cli 二进制名
pub const LARK_CLI_BIN: &str = "lark-cli";

/// 默认超时 60s
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// 默认输出截断上限 1MB
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

// ==================== 凭据需求声明（工厂与实例共用单点，D17） ====================

/// 凭据需求静态声明：同一 LarkApp 凭证三字段（D4 多字段模式；
/// readiness 判定与 call_tool 编排经工厂读取，check 注入经实例读取）
fn credential_requirements() -> Vec<CredentialRequirement> {
    fn internal(field: &str) -> CredentialRequirement {
        CredentialRequirement {
            kind: CredentialKind::LarkApp,
            platform: None,
            field: Some(field.to_string()),
            binding: CredentialBinding::Internal {
                field: field.to_string(),
            },
            enhancer: None,
        }
    }
    vec![
        internal("app_id"),
        internal("app_secret"),
        internal("identity_mode"),
    ]
}

// ==================== 工具定义 ====================

/// lark_cli 工具配置（存储于 `ToolPo.config`）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LarkCliConfig {
    /// lark-cli 二进制名或绝对路径（缺省 `LARK_CLI_BIN`；存量 config 无该字段 → 常量兜底，D28）
    pub command: Option<String>,
    /// 默认超时（毫秒）
    pub default_timeout_ms: Option<u64>,
    /// 默认输出截断上限（字节）
    pub default_max_output_size_bytes: Option<u64>,
}

impl LarkCliConfig {
    /// lark-cli 命令（缺省 `LARK_CLI_BIN` 兜底）
    pub fn command(&self) -> String {
        self.command
            .clone()
            .unwrap_or_else(|| LARK_CLI_BIN.to_string())
    }

    /// 默认超时（毫秒）
    pub fn default_timeout_ms(&self) -> u64 {
        self.default_timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
    }

    /// 默认输出截断上限（字节）
    pub fn default_max_output_size_bytes(&self) -> u64 {
        self.default_max_output_size_bytes
            .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES)
    }
}

/// `lark_cli` 工具参数
#[derive(Debug, Deserialize)]
pub struct LarkCliParams {
    /// lark-cli 子命令与参数（不含二进制名），如 `"calendar +agenda"`
    pub command: String,
    /// 超时（毫秒，覆盖默认 60s）
    pub timeout_ms: Option<u64>,
}

/// lark_cli 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct LarkCliToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for LarkCliToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "lark_cli".to_string(),
            name: "lark_cli".to_string(),
            description: concat!(
                "Execute lark-cli commands to operate Feishu/Lark capabilities ",
                "(messages, docs, calendar, bitable, tasks, etc.) under the bound Feishu app identity. ",
                "Credentials are resolved from the caller's bound Lark channel and never passed as arguments. ",
                "Example commands: 'im +send', 'calendar +agenda', 'task +create'."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "lark-cli subcommand and arguments (without the binary name), e.g. 'calendar +agenda' or 'im +send --user ou_xxx --text hello'."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: timeout in milliseconds. Default: 60000 (1 minute)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            // CLI 命令进 PO config（D28：缺省 LARK_CLI_BIN，工具管理页可改命令路径）
            config: serde_json::json!(LarkCliConfig {
                command: Some(LARK_CLI_BIN.to_string()),
                ..Default::default()
            }),
            tags: serde_json::to_string(&vec!["lark".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(LarkCliCoreTool::new(po))
    }

    /// 凭据需求静态声明（readiness 判定与 call_tool 编排共用，D17）
    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }
}

/// lark_cli 工具核心实现
#[derive(Debug, Clone)]
pub struct LarkCliCoreTool {
    po: ToolPo,
    config: LarkCliConfig,
    /// check 注入的飞书应用凭证三元组 `(app_id, app_secret, identity_mode)`
    ///（D22 create → check → call；None → 绑定引导）
    credentials: Option<(String, String, String)>,
}

impl LarkCliCoreTool {
    fn new(po: ToolPo) -> Self {
        let config = if po.config.is_null() {
            LarkCliConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self {
            po,
            config,
            credentials: None,
        }
    }
}

/// 计算用户隔离的 lark-cli HOME 目录（`{base}/users/{user_id}`）
///
/// 委托 `crate::pkg::paths::user_home`（用户维度统一 HOME 约定），
/// lark-cli 自身配置落在 `{home}/.lark-cli/` 下。
pub fn lark_home(base_data_path: &Path, user_id: &str) -> PathBuf {
    crate::pkg::paths::user_home(base_data_path, user_id)
}

/// 探测二进制是否在 PATH 中可用
pub fn binary_available(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(name).is_file())
}

/// 输出脱敏：包含 token/secret 类关键字的行整行替换为 `[REDACTED]`
pub fn sanitize_lark_output(output: &str) -> String {
    const SENSITIVE_KEYWORDS: &[&str] = &[
        "access_token",
        "app_secret",
        "secret_key",
        "tenant_token",
        "authorization",
    ];
    output
        .lines()
        .map(|line| {
            let lower = line.to_lowercase();
            if SENSITIVE_KEYWORDS.iter().any(|k| lower.contains(k)) {
                "[REDACTED]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 首次幂等写入 lark-cli config（secret 走 stdin，避免出现在进程参数列表）
pub async fn ensure_cli_config(home_dir: &Path, app_id: &str, app_secret: &str) -> Result<()> {
    let config_path = home_dir.join(".lark-cli").join("config.json");
    if config_path.exists() {
        return Ok(());
    }
    tokio::fs::create_dir_all(home_dir).await?;
    let mut child = Command::new(LARK_CLI_BIN)
        .args([
            "config",
            "init",
            "--app-id",
            app_id,
            "--app-secret-stdin",
            "--brand",
            "feishu",
        ])
        .env("HOME", home_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn lark-cli config init: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(app_secret.as_bytes()).await?;
    }
    let status = child.wait().await?;
    if !status.success() {
        return Err(anyhow!(
            "lark-cli config init exited with status {:?}",
            status.code()
        )
        .into());
    }
    Ok(())
}

/// 清除用户 HOME 下的 lark-cli config（凭证变更后重建用）
///
/// 删除 `.lark-cli` 目录（含 config 与身份模式 marker），
/// 下次 lark_cli 执行时由 `ensure_cli_config` 按新凭证重建；
/// 用户授权 token 也存于该目录，仅凭证替换场景调用。
pub async fn clear_cli_config(home_dir: &Path) -> Result<()> {
    let config_dir = home_dir.join(".lark-cli");
    if config_dir.exists() {
        tokio::fs::remove_dir_all(&config_dir).await?;
    }
    Ok(())
}

/// 幂等设置 lark-cli 身份模式（`config default-as <mode>`）
///
/// HOME 下 `.lark-cli/.default_as_marker` 记录当前已生效的模式，
/// 一致则跳过，避免每次工具调用多 spawn 一次子进程。
pub async fn ensure_default_as(home_dir: &Path, mode: &str) -> Result<()> {
    let marker = home_dir.join(".lark-cli").join(".default_as_marker");
    if let Ok(current) = tokio::fs::read_to_string(&marker).await
        && current.trim() == mode
    {
        return Ok(());
    }
    let status = Command::new(LARK_CLI_BIN)
        .args(["config", "default-as", mode])
        .env("HOME", home_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| anyhow!("Failed to spawn lark-cli config default-as: {}", e))?;
    if !status.success() {
        return Err(anyhow!(
            "lark-cli config default-as exited with status {:?}",
            status.code()
        )
        .into());
    }
    tokio::fs::write(&marker, mode).await?;
    Ok(())
}

#[async_trait::async_trait]
impl CoreTool for LarkCliCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        let params: LarkCliParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        if params.command.trim().is_empty() {
            return Ok(serde_json::json!({
                "success": false,
                "error": "command 不能为空"
            }));
        }

        // 1. 取 check 注入的凭证（未注入 → 绑定引导；正常编排在 domain 层
        //    resolve 阶段已出引导，此处为直调/漏 check 的防御路径）
        let Some((app_id, app_secret, identity_mode)) = self.credentials.clone() else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "请先在个人设置的飞书集成中绑定应用，并创建引用该凭证的 Lark 渠道"
            }));
        };

        // 2. HOME 隔离 + 首次幂等写入 config
        let Some(user_id) = ctx.user_id.clone() else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "当前上下文缺少用户身份，无法解析 lark-cli 凭证"
            }));
        };
        let home_dir = lark_home(&get().base_data_path(), &user_id);
        // 命令读实例 PO config（存量 config 无 command → LARK_CLI_BIN 常量兜底，D28）
        let bin = self.config.command();
        if !tool_readiness::command_available(&bin) {
            return Ok(tool_readiness::cli_not_installed_json(
                "lark-cli",
                "安装 lark-cli：https://github.com/larksuite/lark-cli",
                "或确认 lark-cli 已安装且在服务进程的 PATH 中，或在工具配置中修改命令路径",
            ));
        }
        ensure_cli_config(&home_dir, &app_id, &app_secret).await?;
        // 幂等对齐渠道身份模式（auto/bot/user，缺省 auto）
        ensure_default_as(&home_dir, &identity_mode).await?;

        // 3. spawn lark-cli 子进程（不经 shell，直接按空白切分参数）
        let cli_args: Vec<String> = params
            .command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| self.config.default_timeout_ms());
        let max_output_bytes = self.config.default_max_output_size_bytes();

        let mut command = Command::new(&bin);
        command.args(&cli_args);
        command.env("HOME", &home_dir);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        // 超时丢弃 future 时同步终止子进程
        command.kill_on_drop(true);

        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to spawn lark-cli: {}", e)
                }));
            }
        };

        let timeout = std::time::Duration::from_millis(timeout_ms);
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = if stderr.trim().is_empty() {
                    stdout.to_string()
                } else {
                    format!("{}\n[stderr]\n{}", stdout, stderr)
                };
                let sanitized = sanitize_lark_output(&combined);
                let truncated = sanitized.len() as u64 > max_output_bytes;
                let summary = if truncated {
                    format!(
                        "{}\n\n... [truncated]",
                        &sanitized[..max_output_bytes as usize]
                    )
                } else {
                    sanitized
                };
                Ok(serde_json::json!({
                    "success": output.status.success(),
                    "exit_code": output.status.code(),
                    "truncated": truncated,
                    "output": summary
                }))
            }
            Ok(Err(e)) => Ok(serde_json::json!({
                "success": false,
                "error": format!("lark-cli execution failed: {}", e)
            })),
            Err(_) => {
                // timeout 丢弃 wait_with_output future，kill_on_drop 已终止子进程
                Ok(serde_json::json!({
                    "success": false,
                    "timeout": true,
                    "timeout_ms": timeout_ms,
                    "error": format!("lark-cli timed out after {} ms and was killed", timeout_ms)
                }))
            }
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }

    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }

    fn check(
        &mut self,
        resolved: &[crate::pkg::credential::ResolvedRequirement],
    ) -> Result<()> {
        let (mut app_id, mut app_secret, mut identity_mode) = (None, None, None);
        for item in resolved {
            match &item.requirement.binding {
                // 内置工具唯一合法注入点（静态声明已限定，此处防御兜底）
                CredentialBinding::Internal { field } if field == "app_id" => {
                    app_id = Some(item.value.clone());
                }
                CredentialBinding::Internal { field } if field == "app_secret" => {
                    app_secret = Some(item.value.clone());
                }
                CredentialBinding::Internal { field } if field == "identity_mode" => {
                    identity_mode = Some(item.value.clone());
                }
                _ => {
                    return Err(err!(
                        InvalidRequest,
                        "lark_cli 仅支持 app_id/app_secret/identity_mode 内部凭据注入点"
                    ));
                }
            }
        }
        // 三字段全量到齐才置位（部分注入 → 保持 None，call 出绑定引导）
        if let (Some(a), Some(s), Some(m)) = (app_id, app_secret, identity_mode) {
            self.credentials = Some((a, s, m));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;
    use crate::pkg::tool_registry::BuiltinToolFactory;

    /// 测试用 RequestContext（懒连接内存 SQLite，不产生真实 IO）
    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
    }

    #[test]
    fn factory_po_metadata() {
        let po = LarkCliToolFactory.create_po();
        assert_eq!(po.id, "lark_cli");
        assert_eq!(po.control_mode, ControlMode::Auto);
        assert_eq!(po.protocol, ToolProtocol::Builtin);
        assert_eq!(po.get_tags(), vec!["lark"]);
        // CLI 命令进 PO config（D28 不变式：CLI 型 = po.config.command）
        assert_eq!(po.cli_command().as_deref(), Some(LARK_CLI_BIN));
    }

    #[test]
    fn lark_home_path_is_user_isolated() {
        let home = lark_home(Path::new("/data/.ai_orz"), "user-001");
        assert_eq!(home, PathBuf::from("/data/.ai_orz/users/user-001"));
    }

    #[test]
    fn sanitize_output_redacts_token_lines() {
        let input = "hello\n{\"tenant_access_token\":\"t-abc\"}\nsafe line\napp_secret: xxx";
        let out = sanitize_lark_output(input);
        assert!(out.contains("hello"));
        assert!(out.contains("safe line"));
        assert!(!out.contains("t-abc"));
        assert!(!out.contains("xxx"));
        assert_eq!(out.matches("[REDACTED]").count(), 2);
    }

    #[test]
    fn config_defaults() {
        let config = LarkCliConfig::default();
        assert_eq!(config.default_timeout_ms(), 60_000);
        assert_eq!(config.default_max_output_size_bytes(), 1024 * 1024);
    }

    #[tokio::test]
    async fn call_without_check_returns_guidance() {
        // 未 check 注入（credentials 字段 None）→ 绑定引导
        //（正常编排在 domain 层出引导，此处为直调/漏 check 的防御路径）
        let tool = LarkCliCoreTool::new(LarkCliToolFactory.create_po());
        let ctx = test_ctx();
        let result = tool
            .call(ctx, serde_json::json!({ "command": "calendar +agenda" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("飞书集成中绑定应用"));
    }

    #[test]
    fn check_injects_credentials_triple_from_resolved_requirements() {
        let mut tool = LarkCliCoreTool::new(LarkCliToolFactory.create_po());
        assert_eq!(tool.credentials, None);
        let resolved: Vec<crate::pkg::credential::ResolvedRequirement> =
            credential_requirements()
                .into_iter()
                .map(|requirement| crate::pkg::credential::ResolvedRequirement {
                    value: match &requirement.field {
                        Some(field) if field == "app_id" => "cli_test".to_string(),
                        Some(field) if field == "app_secret" => "sec_test".to_string(),
                        _ => "tenant".to_string(),
                    },
                    requirement,
                })
                .collect();
        tool.check(&resolved).unwrap();
        assert_eq!(
            tool.credentials,
            Some((
                "cli_test".to_string(),
                "sec_test".to_string(),
                "tenant".to_string()
            ))
        );
    }

    #[test]
    fn factory_and_instance_requirements_are_consistent() {
        // 工厂声明（readiness/编排预判）与实例声明（DAL check 流程）同源，防漂移
        let tool = LarkCliCoreTool::new(LarkCliToolFactory.create_po());
        assert_eq!(
            LarkCliToolFactory.credential_requirements(),
            tool.credential_requirements()
        );
        let requirements = tool.credential_requirements();
        assert_eq!(requirements.len(), 3);
        assert!(requirements
            .iter()
            .all(|r| r.kind == CredentialKind::LarkApp));
    }

    #[tokio::test]
    async fn call_with_empty_command_returns_error_json() {
        let tool = LarkCliCoreTool::new(LarkCliToolFactory.create_po());
        let ctx = test_ctx();
        let result = tool.call(ctx, serde_json::json!({ "command": "  " })).await;
        if let Ok(v) = result {
            assert_eq!(v["success"], false);
            assert!(v["error"].as_str().unwrap().contains("command"));
        }
    }
}
