//! Builtin gh_cli tool implementation
//!
//! 通过 `gh` 子进程让 Agent 操作 GitHub 能力（repo/issue/pr/release 等）。
//!
//! # 凭证与隔离
//!
//! - 凭证不在工具入参中传递：按 `ctx.user_id` 经 `GhCredentialResolver` 查该用户
//!   凭证库中的 GitHub token（未绑定返回引导性错误）
//! - HOME 隔离：每次执行注入 `HOME={base_data_path}/users/{user_id}`（用户维度统一 HOME，
//!   见 `pkg::paths::user_home`），gh 配置落在 `{home}/.config/gh/`
//! - 首次幂等登录：marker 记录 token 摘要，token 变更时自动重新
//!   `gh auth login --with-token`（token 走 stdin，避免进程参数泄露）并
//!   `gh auth setup-git`（同 HOME 下 git push/pull 复用 gh 凭证）
//! - 工作目录：默认落在调用身份对应的工作区（见 `pkg::paths::default_workspace`），
//!   Agent 为用户执行任务时即 `users/{uid}/agents/{aid}/work`
//! - 输出脱敏：返回摘要中 token/secret 类关键字行与 gh token 前缀行整行过滤
//!
//! # 分层说明
//!
//! `GhCredentialResolver` trait 定义在 pkg 层（无上层依赖），具体实现由
//! user DAL 提供并在 `service::init` 注册，工具不直连 DAL/DAO。

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// gh 二进制名
pub const GH_CLI_BIN: &str = "gh";

/// 默认超时 60s
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// 默认输出截断上限 1MB
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

// ==================== 凭证解析器 ====================

/// GitHub token 凭证解析器（pkg 层抽象，由上层实现并注册）
#[async_trait::async_trait]
pub trait GhCredentialResolver: Send + Sync {
    /// 解析当前上下文用户的 GitHub token（已解密）；未绑定返回 None
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<String>>;
}

static RESOLVER: OnceLock<Box<dyn GhCredentialResolver>> = OnceLock::new();

/// 注册全局凭证解析器（service::init 阶段调用，仅首次生效）
pub fn set_credential_resolver(resolver: Box<dyn GhCredentialResolver>) {
    let _ = RESOLVER.set(resolver);
}

/// 获取已注册的全局凭证解析器
pub fn get_credential_resolver() -> Option<&'static dyn GhCredentialResolver> {
    RESOLVER.get().map(|r| r.as_ref())
}

// ==================== 工具定义 ====================

/// gh_cli 工具配置（存储于 `ToolPo.config`）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GhCliConfig {
    /// 默认超时（毫秒）
    pub default_timeout_ms: Option<u64>,
    /// 默认输出截断上限（字节）
    pub default_max_output_size_bytes: Option<u64>,
}

impl GhCliConfig {
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

/// `gh_cli` 工具参数
#[derive(Debug, Deserialize)]
pub struct GhCliParams {
    /// gh 子命令与参数（不含二进制名），如 `"repo list --limit 20"`
    pub command: String,
    /// 超时（毫秒，覆盖默认 60s）
    pub timeout_ms: Option<u64>,
    /// 工作目录（缺省为调用身份对应的默认工作区；相对路径按 base_data_path 解析）
    pub working_dir: Option<String>,
}

/// gh_cli 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct GhCliToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for GhCliToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "gh_cli".to_string(),
            name: "gh_cli".to_string(),
            description: concat!(
                "Execute GitHub CLI (gh) commands to operate GitHub capabilities ",
                "(repos, issues, pull requests, releases, actions, etc.) under the caller's bound ",
                "GitHub token identity. The token is resolved from the user's bound credentials and ",
                "never passed as arguments. Runs inside the caller's isolated workspace. ",
                "Example commands: 'repo list --limit 20', 'issue list -R owner/repo', ",
                "'pr view 123 -R owner/repo', 'repo clone owner/repo'. ",
                "Be careful with destructive commands (e.g. 'repo delete'): they are irreversible."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "gh subcommand and arguments (without the binary name), e.g. 'repo list --limit 20' or 'pr create --title fix --body description'."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: timeout in milliseconds. Default: 60000 (1 minute)."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional: working directory for repository-local commands (clone/pr create). Defaults to the caller's workspace."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            config: serde_json::json!(GhCliConfig::default()),
            tags: serde_json::to_string(&vec!["github".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(GhCliCoreTool::new(po))
    }
}

/// gh_cli 工具核心实现
#[derive(Debug, Clone)]
pub struct GhCliCoreTool {
    po: ToolPo,
    config: GhCliConfig,
}

impl GhCliCoreTool {
    fn new(po: ToolPo) -> Self {
        let config = if po.config.is_null() {
            GhCliConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self { po, config }
    }
}

/// 计算用户隔离的 gh HOME 目录（`{base}/users/{user_id}`）
///
/// 委托 `crate::pkg::paths::user_home`（用户维度统一 HOME 约定），
/// gh 自身配置落在 `{home}/.config/gh/` 下。
pub fn gh_home(base_data_path: &Path, user_id: &str) -> PathBuf {
    crate::pkg::paths::user_home(base_data_path, user_id)
}

/// 探测二进制是否在 PATH 中可用
pub fn binary_available(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(name).is_file())
}

/// 输出脱敏：token/secret 类关键字行与 GitHub token 前缀行整行替换为 `[REDACTED]`
pub fn sanitize_gh_output(output: &str) -> String {
    /// GitHub token 常见格式前缀（gho_/ghu_/ghs_/ghr_/ghp_）
    const TOKEN_PREFIXES: &[&str] = &["gho_", "ghu_", "ghs_", "ghr_", "ghp_"];
    const SENSITIVE_KEYWORDS: &[&str] = &["token", "authorization", "secret", "password"];
    output
        .lines()
        .map(|line| {
            let lower = line.to_lowercase();
            let sensitive = SENSITIVE_KEYWORDS.iter().any(|k| lower.contains(k))
                || TOKEN_PREFIXES.iter().any(|p| line.contains(p));
            if sensitive { "[REDACTED]" } else { line }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// token 摘要 marker 路径（记录当前已落盘登录 token 的指纹）
fn token_marker_path(home_dir: &Path) -> PathBuf {
    home_dir
        .join(".config")
        .join("gh")
        .join(".ai_orz_token_marker")
}

/// 计算 token 摘要（sha256 前 16 位 hex，仅作变更检测指纹，不可逆推 token）
fn token_fingerprint(token: &str) -> String {
    sha256::digest(token.as_bytes())[..16].to_string()
}

/// 首次幂等登录 gh（token 走 stdin，避免出现在进程参数列表）
///
/// marker 记录 token 指纹：一致跳过；不一致（token 轮换）重新登录并
/// `gh auth setup-git`（同 HOME 下 git 复用 gh 凭证）。
pub async fn ensure_gh_auth(home_dir: &Path, token: &str) -> Result<()> {
    let marker = token_marker_path(home_dir);
    let fingerprint = token_fingerprint(token);
    if let Ok(current) = tokio::fs::read_to_string(&marker).await
        && current.trim() == fingerprint
    {
        return Ok(());
    }
    tokio::fs::create_dir_all(home_dir).await?;
    let mut child = Command::new(GH_CLI_BIN)
        .args(["auth", "login", "--with-token"])
        .env("HOME", home_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn gh auth login: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(token.as_bytes()).await?;
        stdin.write_all(b"\n").await.ok();
    }
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        // 失败信息本身可能含 token 片段，统一脱敏后再返回
        let stderr = sanitize_gh_output(&String::from_utf8_lossy(&output.stderr));
        return Err(anyhow!(
            "gh auth login failed (status {:?}): {}",
            output.status.code(),
            stderr
        )
        .into());
    }
    // git 复用 gh 凭证（幂等，写用户 HOME 下 gitconfig 的 credential helper）
    let setup = Command::new(GH_CLI_BIN)
        .args(["auth", "setup-git"])
        .env("HOME", home_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| anyhow!("Failed to spawn gh auth setup-git: {}", e))?;
    if !setup.success() {
        return Err(anyhow!("gh auth setup-git exited with status {:?}", setup.code()).into());
    }
    tokio::fs::write(&marker, &fingerprint).await?;
    Ok(())
}

/// 清除用户 HOME 下的 gh 登录态（凭证删除/登出用）
///
/// 删除 `{home}/.config/gh/hosts.yml` 与 token marker，
/// 下次 gh_cli 执行时由 `ensure_gh_auth` 按当前凭证重建。
pub async fn clear_gh_auth(home_dir: &Path) -> Result<()> {
    let hosts = home_dir.join(".config").join("gh").join("hosts.yml");
    if hosts.exists() {
        tokio::fs::remove_file(&hosts).await?;
    }
    let marker = token_marker_path(home_dir);
    if marker.exists() {
        tokio::fs::remove_file(&marker).await?;
    }
    Ok(())
}

/// gh 登录态快照（gh_auth_status 探测结果）
#[derive(Debug, Clone, Default)]
pub struct GhAuthStatus {
    /// HOME 下 gh 是否已登录（有可用账号）
    pub logged_in: bool,
    /// 已登录 GitHub 账号名
    pub user_name: Option<String>,
    /// 引导提示（gh 未安装 / 输出不可解析等，不构成错误）
    pub hint: Option<String>,
}

/// 探测用户 HOME 下的 gh 登录态（`gh auth status --json`）
///
/// 未登录时 gh 退出码非 0，属正常态而非错误；
/// gh 二进制缺失时返回带 hint 的未登录快照（与 lark auth status 降级模式一致）。
pub async fn gh_auth_status(home_dir: &Path) -> GhAuthStatus {
    if !binary_available(GH_CLI_BIN) {
        return GhAuthStatus {
            logged_in: false,
            user_name: None,
            hint: Some("未找到 gh 二进制，请先安装：https://cli.github.com".to_string()),
        };
    }
    let output = match Command::new(GH_CLI_BIN)
        .args(["auth", "status", "--json", "accounts"])
        .env("HOME", home_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return GhAuthStatus {
                hint: Some(format!("gh auth status 执行失败: {}", e)),
                ..Default::default()
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 已登录判定：accounts 数组非空（未登录时 gh 退出码非 0 且 accounts 缺失/为空）
    let parsed: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
    let account = parsed
        .as_ref()
        .and_then(|v| v.get("accounts"))
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|acc| acc.get("account"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    GhAuthStatus {
        logged_in: account.is_some(),
        user_name: account,
        hint: None,
    }
}

#[async_trait::async_trait]
impl CoreTool for GhCliCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        let params: GhCliParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        if params.command.trim().is_empty() {
            return Ok(serde_json::json!({
                "success": false,
                "error": "command 不能为空"
            }));
        }

        // 1. 解析凭证（未绑定 → 引导性错误）
        let Some(resolver) = get_credential_resolver() else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "gh_cli 凭证解析器未就绪，请重启服务后重试"
            }));
        };
        let Some(token) = resolver.resolve(&ctx).await? else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "请先在个人设置的 GitHub 集成中绑定访问令牌（Personal Access Token）"
            }));
        };

        // 2. HOME 隔离 + 首次幂等登录
        let Some(user_id) = ctx.user_id.clone() else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "当前上下文缺少用户身份，无法解析 GitHub 凭证"
            }));
        };
        let base_path = get().base_data_path();
        let home_dir = gh_home(&base_path, &user_id);
        if !binary_available(GH_CLI_BIN) {
            return Ok(serde_json::json!({
                "success": false,
                "error": "未找到 gh 二进制，请先安装：https://cli.github.com"
            }));
        }
        ensure_gh_auth(&home_dir, &token).await?;

        // 3. 工作目录（缺省按调用身份选默认工作区）
        let working_dir = match params.working_dir.as_deref() {
            Some(path) if !path.trim().is_empty() && Path::new(path).is_absolute() => {
                PathBuf::from(path)
            }
            Some(path) if !path.trim().is_empty() => Path::new(&base_path).join(path),
            _ => crate::pkg::paths::default_workspace(
                &base_path,
                ctx.user_id.as_deref(),
                ctx.agent_id.as_deref(),
            ),
        };
        tokio::fs::create_dir_all(&working_dir).await.ok();

        // 4. spawn gh 子进程（不经 shell，直接按空白切分参数）
        let cli_args: Vec<String> = params
            .command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| self.config.default_timeout_ms());
        let max_output_bytes = self.config.default_max_output_size_bytes();

        let mut command = Command::new(GH_CLI_BIN);
        command
            .args(&cli_args)
            .env("HOME", &home_dir)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // 超时丢弃 future 时同步终止子进程
            .kill_on_drop(true);

        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to spawn gh: {}", e)
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
                let sanitized = sanitize_gh_output(&combined);
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
                    "working_dir": working_dir.to_string_lossy(),
                    "output": summary
                }))
            }
            Ok(Err(e)) => Ok(serde_json::json!({
                "success": false,
                "error": format!("gh execution failed: {}", e)
            })),
            Err(_) => {
                // timeout 丢弃 wait_with_output future，kill_on_drop 已终止子进程
                Ok(serde_json::json!({
                    "success": false,
                    "timeout": true,
                    "timeout_ms": timeout_ms,
                    "error": format!("gh timed out after {} ms and was killed", timeout_ms)
                }))
            }
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
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
        let po = GhCliToolFactory.create_po();
        assert_eq!(po.id, "gh_cli");
        assert_eq!(po.control_mode, ControlMode::Auto);
        assert_eq!(po.protocol, ToolProtocol::Builtin);
        assert_eq!(po.get_tags(), vec!["github"]);
    }

    #[test]
    fn gh_home_path_is_user_isolated() {
        let home = gh_home(Path::new("/data/.ai_orz"), "user-001");
        assert_eq!(home, PathBuf::from("/data/.ai_orz/users/user-001"));
    }

    #[test]
    fn sanitize_output_redacts_token_lines() {
        let input = "repo list ok\nToken: ghp_abcdef123456\nsafe line\nauthorization: Bearer x\ngho_secret\npassword: p";
        let out = sanitize_gh_output(input);
        assert!(out.contains("repo list ok"));
        assert!(out.contains("safe line"));
        assert!(!out.contains("ghp_abcdef123456"));
        assert!(!out.contains("Bearer x"));
        assert!(!out.contains("gho_secret"));
        assert!(!out.contains("password: p"));
        assert_eq!(out.matches("[REDACTED]").count(), 4);
    }

    #[test]
    fn config_defaults() {
        let config = GhCliConfig::default();
        assert_eq!(config.default_timeout_ms(), 60_000);
        assert_eq!(config.default_max_output_size_bytes(), 1024 * 1024);
    }

    #[test]
    fn token_fingerprint_is_stable_and_short() {
        let a = token_fingerprint("ghp_same-token");
        assert_eq!(a, token_fingerprint("ghp_same-token"));
        assert_ne!(a, token_fingerprint("ghp_other-token"));
        assert_eq!(a.len(), 16);
    }

    #[tokio::test]
    async fn call_without_resolver_returns_error_json() {
        // 单测环境未注册 resolver（OnceLock 全局只设一次，此处依赖未注册状态）
        if get_credential_resolver().is_some() {
            return;
        }
        let tool = GhCliCoreTool::new(GhCliToolFactory.create_po());
        let ctx = test_ctx();
        let result = tool
            .call(ctx, serde_json::json!({ "command": "repo list" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("凭证解析器"));
    }

    #[tokio::test]
    async fn call_with_empty_command_returns_error_json() {
        let tool = GhCliCoreTool::new(GhCliToolFactory.create_po());
        let ctx = test_ctx();
        let result = tool.call(ctx, serde_json::json!({ "command": "  " })).await;
        if let Ok(v) = result {
            assert_eq!(v["success"], false);
            assert!(v["error"].as_str().unwrap().contains("command"));
        }
    }
}
