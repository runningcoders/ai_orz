//! Builtin gh_cli tool implementation
//!
//! 通过 `gh` 子进程让 Agent 操作 GitHub 能力（repo/issue/pr/release 等）。
//!
//! # 凭证与隔离
//!
//! - 凭证不在工具入参中传递：凭据需求由工厂静态声明（个人 GitHub token），
//!   domain 编排层（`resolve_tool_credentials`）据此取用户凭证，经
//!   `CoreTool::check` 注入实例 `token` 字段（D17 工厂化，未注入 → 绑定引导）
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
//! 凭据取数在 domain 编排层（D17 v1.5）：pkg 只保留纯函数与静态需求声明
//! （工厂与实例共用单点），工具实例不直连 DAL/DAO。

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::process::{ExecOptions, exec};
use crate::pkg::request_context::RequestContext;
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::{Result, err};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// gh 二进制名
pub const GH_CLI_BIN: &str = "gh";

/// 默认超时 60s
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// 默认输出截断上限 1MB
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

// ==================== 凭据需求声明（工厂与实例共用单点，D17） ====================

/// 凭据需求静态声明：个人 GitHub token（单条 Internal 注入实例 `token` 字段；
/// readiness 判定与 call_tool 编排经工厂读取，check 注入经实例读取）
fn credential_requirements() -> Vec<CredentialRequirement> {
    vec![CredentialRequirement {
        kind: CredentialKind::GithubToken,
        platform: None,
        field: None,
        enhancer: None,
        binding: CredentialBinding::Internal {
            field: "token".to_string(),
        },
    }]
}

// ==================== 工具定义 ====================

/// gh_cli 工具配置（存储于 `ToolPo.config`）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GhCliConfig {
    /// gh 二进制名或绝对路径（缺省 `GH_CLI_BIN`；存量 config 无该字段 → 常量兜底，D28）
    pub command: Option<String>,
    /// 默认超时（毫秒）
    pub default_timeout_ms: Option<u64>,
    /// 默认输出截断上限（字节）
    pub default_max_output_size_bytes: Option<u64>,
}

impl GhCliConfig {
    /// gh 命令（缺省 `GH_CLI_BIN` 兜底）
    pub fn command(&self) -> String {
        self.command
            .clone()
            .unwrap_or_else(|| GH_CLI_BIN.to_string())
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
            name: "GitHub CLI".to_string(),
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
            // CLI 命令进 PO config（D28：缺省 GH_CLI_BIN，工具管理页可改命令路径）
            config: serde_json::json!(GhCliConfig {
                command: Some(GH_CLI_BIN.to_string()),
                ..Default::default()
            }),
            tags: serde_json::to_string(&vec!["github".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(GhCliCoreTool::new(po))
    }

    /// 凭据需求静态声明（readiness 判定与 call_tool 编排共用，D17）
    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }
}

/// gh_cli 工具核心实现
#[derive(Debug, Clone)]
pub struct GhCliCoreTool {
    po: ToolPo,
    config: GhCliConfig,
    /// check 注入的 GitHub token（D22 create → check → call；None → 绑定引导）
    token: Option<String>,
}

impl GhCliCoreTool {
    fn new(po: ToolPo) -> Self {
        let config = if po.config.is_null() {
            GhCliConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self {
            po,
            config,
            token: None,
        }
    }
}

/// 计算用户隔离的 gh HOME 目录（`{base}/users/{user_id}`）
///
/// 委托 `crate::pkg::paths::user_home`（用户维度统一 HOME 约定），
/// gh 自身配置落在 `{home}/.config/gh/` 下。
pub fn gh_home(base_data_path: &Path, user_id: &str) -> PathBuf {
    crate::pkg::paths::user_home(base_data_path, user_id)
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
    let output = exec(
        &ExecOptions::new(
            GH_CLI_BIN,
            vec!["auth".into(), "login".into(), "--with-token".into()],
        )
        .env("HOME", home_dir.to_string_lossy().to_string())
        // token 走 stdin，避免进程参数泄露
        .stdin([token.as_bytes(), b"\n"].concat()),
    )
    .await
    .map_err(|e| anyhow!("Failed to spawn gh auth login: {}", e))?;
    if !output.success {
        // 失败信息本身可能含 token 片段，统一脱敏后再返回
        let stderr = sanitize_gh_output(&String::from_utf8_lossy(&output.stderr));
        return Err(anyhow!(
            "gh auth login failed (status {:?}): {}",
            output.exit_code,
            stderr
        )
        .into());
    }
    // git 复用 gh 凭证（幂等，写用户 HOME 下 gitconfig 的 credential helper）
    let setup = exec(
        &ExecOptions::new(GH_CLI_BIN, vec!["auth".into(), "setup-git".into()])
            .env("HOME", home_dir.to_string_lossy().to_string()),
    )
    .await
    .map_err(|e| anyhow!("Failed to spawn gh auth setup-git: {}", e))?;
    if !setup.success {
        return Err(anyhow!("gh auth setup-git exited with status {:?}", setup.exit_code).into());
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
    if !crate::pkg::tool_registry::tool_readiness::command_available(GH_CLI_BIN) {
        return GhAuthStatus {
            logged_in: false,
            user_name: None,
            hint: Some("未找到 gh 二进制，请先安装：https://cli.github.com".to_string()),
        };
    }
    let output = match exec(
        &ExecOptions::new(
            GH_CLI_BIN,
            vec![
                "auth".into(),
                "status".into(),
                "--json".into(),
                "accounts".into(),
            ],
        )
        .env("HOME", home_dir.to_string_lossy().to_string()),
    )
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
    // 探测路径曾无超时（可挂死）；exec 原语默认 60s 兜底
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

        // 1. 取 check 注入的凭证（未注入 → 绑定引导；正常编排在 domain 层
        //    resolve 阶段已出引导，此处为直调/漏 check 的防御路径）
        let Some(token) = self.token.clone() else {
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
        // 命令读实例 PO config（存量 config 无 command → GH_CLI_BIN 常量兜底，D28）
        let bin = self.config.command();
        if !crate::pkg::tool_registry::tool_readiness::command_available(&bin) {
            return Ok(
                crate::pkg::tool_registry::tool_readiness::cli_not_installed_json(
                    "gh",
                    "安装 GitHub CLI：https://cli.github.com（brew install gh）",
                    "或确认 gh 已安装且在服务进程的 PATH 中，或在工具配置中修改命令路径",
                ),
            );
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

        let output = match exec(
            &ExecOptions::new(&bin, cli_args)
                .env("HOME", home_dir.to_string_lossy().to_string())
                .current_dir(&working_dir)
                .timeout(std::time::Duration::from_millis(timeout_ms)),
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to spawn gh: {}", e)
                }));
            }
        };

        if output.timed_out {
            // exec 原语已在超时时终止子进程
            return Ok(serde_json::json!({
                "success": false,
                "timeout": true,
                "timeout_ms": timeout_ms,
                "error": format!("gh timed out after {} ms and was killed", timeout_ms)
            }));
        }

        {
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
                "success": output.success,
                "exit_code": output.exit_code,
                "truncated": truncated,
                "working_dir": working_dir.to_string_lossy(),
                "output": summary
            }))
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }

    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }

    fn check(&mut self, resolved: &[crate::pkg::credential::ResolvedRequirement]) -> Result<()> {
        for item in resolved {
            match &item.requirement.binding {
                // 内置工具唯一合法注入点（静态声明已限定，此处防御兜底）
                CredentialBinding::Internal { field } if field == "token" => {
                    self.token = Some(item.value.clone());
                }
                _ => {
                    return Err(err!(InvalidRequest, "gh_cli 仅支持 token 内部凭据注入点"));
                }
            }
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
        let po = GhCliToolFactory.create_po();
        assert_eq!(po.id, "gh_cli");
        assert_eq!(po.control_mode, ControlMode::Auto);
        assert_eq!(po.protocol, ToolProtocol::Builtin);
        assert_eq!(po.get_tags(), vec!["github"]);
        // CLI 命令进 PO config（D28 不变式：CLI 型 = po.config.command）
        assert_eq!(po.cli_command().as_deref(), Some(GH_CLI_BIN));
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
    async fn call_without_check_returns_guidance() {
        // 未 check 注入（token 字段 None）→ 绑定引导（正常编排在 domain 层出引导，此处防御）
        let tool = GhCliCoreTool::new(GhCliToolFactory.create_po());
        let ctx = test_ctx();
        let result = tool
            .call(ctx, serde_json::json!({ "command": "repo list" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("GitHub 集成"));
    }

    #[test]
    fn check_injects_token_from_resolved_requirement() {
        let mut tool = GhCliCoreTool::new(GhCliToolFactory.create_po());
        assert_eq!(tool.token, None);
        let resolved = vec![crate::pkg::credential::ResolvedRequirement {
            requirement: credential_requirements().pop().unwrap(),
            value: "ghp_test_token".to_string(),
        }];
        tool.check(&resolved).unwrap();
        assert_eq!(tool.token.as_deref(), Some("ghp_test_token"));
    }

    #[test]
    fn factory_and_instance_requirements_are_consistent() {
        // 工厂声明（readiness/编排预判）与实例声明（DAL check 流程）同源，防漂移
        let tool = GhCliCoreTool::new(GhCliToolFactory.create_po());
        assert_eq!(
            GhCliToolFactory.credential_requirements(),
            tool.credential_requirements()
        );
        assert_eq!(
            tool.credential_requirements()[0].kind,
            CredentialKind::GithubToken
        );
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
