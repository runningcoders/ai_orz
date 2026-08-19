//! Builtin browser tool implementation（包装 agent-browser CLI）
//!
//! 通过 [agent-browser](https://github.com/vercel-labs/agent-browser) 让 Agent 获得
//! JS 渲染页阅读与页面交互能力，弥补 http_fetch 只能抓静态页的缺口。
//!
//! # 关键边界（设计见 docs/design/web_search_and_browser_tools_design.md）
//!
//! - **Manual 模式**：浏览器可产生真实网络副作用（登录/提交/下载），需人工确认
//! - **子命令白名单**：仅开放原子操作（open/read/snapshot/click/fill/...）；
//!   脚本执行类（eval/batch/keyboard 等）与状态泄露类（cookies/storage）不开放
//! - **会话隔离**：固定 `--session ai-orz-agent-{agent_id}`，Agent 间互不干扰，
//!   daemon 常驻架构下页面状态跨调用保持
//! - **固定 headless**：不向 Agent 暴露 headed/可视化参数
//! - **截图产物**：screenshot 输出落统一产物存储（经 ScreenshotStorer 注入），
//!   返回 { artifact_id, name } 引用，禁止 base64 内嵌
//! - **spawn 不经 shell**：argv 直拼，无注入面；输出超时 + 截断
//!
//! # 分层说明
//!
//! `ScreenshotStorer` trait 定义在 pkg 层（无上层依赖），由 project Domain 实现
//! 并在 `service::init` 注册，工具不直连 DAL/DAO。

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::tool_readiness;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;

/// 子命令白名单（原子操作集；eval/脚本执行类、cookies/storage 泄露类一律不在列）
const ALLOWED_COMMANDS: &[&str] = &[
    // 导航
    "open",
    "back",
    "forward",
    "reload",
    // 阅读（agent 友好）
    "read",
    "snapshot",
    "get",
    // 交互
    "click",
    "dblclick",
    "fill",
    "type",
    "press",
    "hover",
    "select",
    "check",
    "uncheck",
    "scroll",
    // 等待 / 取证 / 生命周期
    "wait",
    "screenshot",
    "close",
];

// ==================== 截图产物存储器 ====================

/// 截图产物引用（落统一产物存储后返回）
#[derive(Debug, Clone)]
pub struct ScreenshotArtifact {
    /// 产物 ID（产物中心可见/可下载）
    pub artifact_id: String,
    /// 产物名称
    pub name: String,
}

/// 截图产物存储器（pkg 层抽象，由上层 Domain 实现并注册）
#[async_trait]
pub trait ScreenshotStorer: Send + Sync {
    /// 将截图文件落统一产物存储，返回产物引用
    async fn store_screenshot(
        &self,
        ctx: RequestContext,
        source_path: PathBuf,
        file_name: String,
    ) -> Result<ScreenshotArtifact>;
}

static SCREENSHOT_STORER: OnceLock<Box<dyn ScreenshotStorer>> = OnceLock::new();

/// 注册全局截图产物存储器（service::init 阶段调用，仅首次生效）
pub fn set_screenshot_storer(storer: Box<dyn ScreenshotStorer>) {
    let _ = SCREENSHOT_STORER.set(storer);
}

/// 获取已注册的全局截图产物存储器
pub fn get_screenshot_storer() -> Option<&'static dyn ScreenshotStorer> {
    SCREENSHOT_STORER.get().map(|s| s.as_ref())
}

// ==================== 工具定义 ====================

/// `browser` 工具参数
#[derive(Debug, Deserialize)]
pub struct BrowserParams {
    /// 子命令（白名单校验，如 open/snapshot/click/fill/screenshot）
    pub command: String,
    /// 子命令参数（如 url、@e1 元素引用、待填文本）
    pub args: Option<Vec<String>>,
    /// 单次超时覆盖（毫秒，默认取 config [browser].timeout_ms）
    pub timeout_ms: Option<u64>,
}

/// browser 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct BrowserToolFactory;

impl BuiltinToolFactory for BrowserToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "browser".to_string(),
            name: "browser".to_string(),
            description: concat!(
                "Automate a headless browser via the agent-browser CLI to read JS-rendered pages ",
                "and interact with web UIs. Typical loop: 'open <url>' to navigate, 'snapshot' to get ",
                "an accessibility tree with @eN element refs, then 'click @eN' / 'fill @eN \"text\"' to ",
                "interact, 'read' for agent-readable page text, 'screenshot' for visual evidence ",
                "(saved as a downloadable artifact). Page state persists across calls within the ",
                "same session. Manual mode: each call requires human confirmation. ",
                "Allowed commands: open/back/forward/reload/read/snapshot/get/click/dblclick/fill/",
                "type/press/hover/select/check/uncheck/scroll/wait/screenshot/close."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Manual,
            parameters_schema: Some(json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "enum": ALLOWED_COMMANDS,
                        "description": "Browser subcommand to execute (whitelist enforced)."
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Subcommand arguments, e.g. [\"https://example.com\"] for open, [\"@e2\"] for click, [\"@e3\", \"text\"] for fill."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: per-call timeout in milliseconds. Default from server config (60s)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            config: Value::Null,
            tags: serde_json::to_string(&vec![
                "browser".to_string(),
                "network".to_string(),
            ])
            .unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(BrowserCoreTool { po })
    }
}

/// browser 工具核心实现
#[derive(Debug, Clone)]
pub struct BrowserCoreTool {
    po: ToolPo,
}

/// 会话 ID：Agent 间隔离（daemon 常驻，页面状态跨调用保持）
fn session_id(ctx: &RequestContext) -> String {
    let owner = ctx
        .agent_id
        .clone()
        .or_else(|| ctx.user_id.clone())
        .unwrap_or_else(|| "default".to_string());
    format!("ai-orz-agent-{}", owner)
}

/// 输出合并（stderr 非空时附加）+ 截断
fn combine_output(stdout: &str, stderr: &str, max_bytes: u64) -> (String, bool) {
    let combined = if stderr.trim().is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n[stderr]\n{}", stdout, stderr)
    };
    if combined.len() as u64 > max_bytes {
        (
            format!("{}\n\n... [truncated]", &combined[..max_bytes as usize]),
            true,
        )
    } else {
        (combined, false)
    }
}

#[async_trait]
impl CoreTool for BrowserCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. 参数解析 + 白名单校验
        let params: BrowserParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let subcommand = params.command.trim().to_lowercase();
        if !ALLOWED_COMMANDS.contains(&subcommand.as_str()) {
            return Ok(json!({
                "success": false,
                "error": format!(
                    "子命令 '{}' 不在白名单内（安全限制）。可用子命令：{}。eval/脚本执行与 cookies/storage 类操作不开放",
                    params.command,
                    ALLOWED_COMMANDS.join("/")
                )
            }));
        }
        let mut cli_args: Vec<String> = params.args.unwrap_or_default();

        // close --all 会波及其他会话，拒绝
        if subcommand == "close" && cli_args.iter().any(|a| a == "--all") {
            return Ok(json!({
                "success": false,
                "error": "close --all 会终止所有会话（含其他 Agent），禁止使用；仅可关闭当前会话"
            }));
        }

        // screenshot：产物路径由系统管理（防 Agent 指定任意写入路径），只允许 flag 参数
        let mut screenshot_path: Option<PathBuf> = None;
        if subcommand == "screenshot" {
            if cli_args.iter().any(|a| !a.starts_with('-')) {
                return Ok(json!({
                    "success": false,
                    "error": "screenshot 无需传路径参数：产物由系统统一落存储；仅接受 --full 等 flag"
                }));
            }
            let path = std::env::temp_dir().join(format!(
                "ai-orz-browser-shot-{}-{}.png",
                chrono::Utc::now().timestamp_millis(),
                ctx.tool_call_id
                    .clone()
                    .unwrap_or_else(|| "anon".to_string())
            ));
            cli_args.push(path.to_string_lossy().to_string());
            screenshot_path = Some(path);
        }

        // 2. CLI 就绪预检（未安装 → 统一安装引导）
        let bin = get().browser.command.clone();
        if !tool_readiness::command_available(&bin) {
            return Ok(tool_readiness::cli_not_installed_json(
                "agent-browser",
                "brew install agent-browser 或 cargo install agent-browser（首次还需执行 agent-browser install 下载 Chrome）",
                "或在 ai_orz.toml 的 [browser].command 配置绝对路径",
            ));
        }

        // 3. spawn（不经 shell，argv 直拼；--session 隔离）
        let session = session_id(&ctx);
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| get().browser.timeout_ms);
        let max_output_bytes = get().browser.max_output_bytes;

        let mut command = Command::new(&bin);
        command
            .arg("--session")
            .arg(&session)
            .arg(&subcommand)
            .args(&cli_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // 超时丢弃 future 时同步终止子进程
            .kill_on_drop(true);

        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                // spawn 错误分类：NotFound → 安装引导；PermissionDenied → 权限提示
                return Ok(match e.kind() {
                    std::io::ErrorKind::NotFound => tool_readiness::cli_not_installed_json(
                        "agent-browser",
                        "brew install agent-browser 或 cargo install agent-browser",
                        "或在 ai_orz.toml 的 [browser].command 配置绝对路径",
                    ),
                    std::io::ErrorKind::PermissionDenied => json!({
                        "success": false,
                        "error": format!("agent-browser 无执行权限（{}），请 chmod +x 或检查路径", e)
                    }),
                    _ => json!({
                        "success": false,
                        "error": format!("启动 agent-browser 失败: {}", e)
                    }),
                });
            }
        };

        // 4. 超时保护 + 输出合并截断
        let timeout = Duration::from_millis(timeout_ms);
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Ok(json!({
                    "success": false,
                    "error": format!("执行 agent-browser 失败: {}", e)
                }));
            }
            Err(_) => {
                return Ok(json!({
                    "success": false,
                    "error": format!("agent-browser 执行超时（{}ms），已终止子进程", timeout_ms),
                    "hint": "页面加载过慢时可分步执行：先 open 再 wait，或调大 timeout_ms 参数"
                }));
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let (combined, truncated) = combine_output(&stdout, &stderr, max_output_bytes);

        let mut payload = json!({
            "success": output.status.success(),
            "command": subcommand,
            "session": session,
            "exit_code": output.status.code(),
            "output": combined,
            "truncated": truncated
        });

        // 5. screenshot 分支：产物落统一存储，返回引用（不内嵌 base64）
        if subcommand == "screenshot" && output.status.success() {
            let path = screenshot_path.expect("screenshot path set above");
            payload["screenshot"] = match store_screenshot(ctx.clone(), &path).await {
                Ok(artifact) => {
                    // 存储成功后清理临时文件
                    let _ = tokio::fs::remove_file(&path).await;
                    json!({
                        "stored": true,
                        "artifact_id": artifact.artifact_id,
                        "name": artifact.name
                    })
                }
                Err(store_err) => {
                    // 存储失败不否定截图本身：保留临时文件路径供手动取用
                    json!({
                        "stored": false,
                        "reason": format!("产物存储失败: {}", store_err),
                        "local_path": path.to_string_lossy(),
                        "hint": "截图已生成但未入产物中心（常见原因：缺少项目上下文）；可在项目任务内重试"
                    })
                }
            };
        }

        Ok(payload)
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// 截图落产物存储（存储器未注册时返回引导错误）
async fn store_screenshot(
    ctx: RequestContext,
    path: &std::path::Path,
) -> Result<ScreenshotArtifact> {
    let Some(storer) = get_screenshot_storer() else {
        return Err(anyhow::anyhow!("截图产物存储器未注册（初始化异常）").into());
    };
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "screenshot.png".to_string());
    storer
        .store_screenshot(ctx, path.to_path_buf(), file_name)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;

    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
    }

    #[test]
    fn factory_po_metadata() {
        let po = BrowserToolFactory.create_po();
        assert_eq!(po.id, "browser");
        assert_eq!(po.control_mode, ControlMode::Manual);
        assert_eq!(po.protocol, ToolProtocol::Builtin);
        assert_eq!(po.get_tags(), vec!["browser", "network"]);
    }

    #[test]
    fn whitelist_excludes_dangerous_commands() {
        for dangerous in [
            "eval", "batch", "chat", "connect", "cookies", "storage", "upload", "network", "set",
            "stream", "keyboard",
        ] {
            assert!(
                !ALLOWED_COMMANDS.contains(&dangerous),
                "{} must not be whitelisted",
                dangerous
            );
        }
        for required in [
            "open",
            "read",
            "snapshot",
            "click",
            "fill",
            "type",
            "press",
            "scroll",
            "screenshot",
            "wait",
            "close",
        ] {
            assert!(
                ALLOWED_COMMANDS.contains(&required),
                "{} must be whitelisted",
                required
            );
        }
    }

    #[tokio::test]
    async fn call_rejects_non_whitelisted_command() {
        let tool = BrowserCoreTool {
            po: BrowserToolFactory.create_po(),
        };
        let result = tool
            .call(
                test_ctx(),
                json!({ "command": "eval", "args": ["alert(1)"] }),
            )
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        let error = result["error"].as_str().unwrap();
        assert!(
            error.contains("eval"),
            "error should name the rejected command: {}",
            error
        );
        assert!(
            error.contains("白名单"),
            "error should mention whitelist: {}",
            error
        );
    }

    #[tokio::test]
    async fn call_rejects_close_all() {
        let tool = BrowserCoreTool {
            po: BrowserToolFactory.create_po(),
        };
        let result = tool
            .call(test_ctx(), json!({ "command": "close", "args": ["--all"] }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("--all"));
    }

    #[tokio::test]
    async fn call_rejects_screenshot_positional_path() {
        let tool = BrowserCoreTool {
            po: BrowserToolFactory.create_po(),
        };
        // Agent 传任意路径（路径穿越风险面）→ 拒绝
        let result = tool
            .call(
                test_ctx(),
                json!({ "command": "screenshot", "args": ["/etc/evil.png"] }),
            )
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("路径"));
    }

    #[tokio::test]
    async fn session_id_isolates_per_agent() {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = crate::pkg::storage::test_support::create_test_storage(pool.clone());
        let ctx_agent = RequestContext::builder()
            .user_id("u-1".to_string())
            .agent_id("agent-9".to_string())
            .storage(storage.clone())
            .build();
        let ctx_user_only = RequestContext::builder()
            .user_id("u-2".to_string())
            .storage(storage)
            .build();
        let ctx_anon = new_test_ctx("", pool);

        assert_eq!(session_id(&ctx_agent), "ai-orz-agent-agent-9");
        assert_eq!(session_id(&ctx_user_only), "ai-orz-agent-u-2");
        assert!(session_id(&ctx_anon).starts_with("ai-orz-agent-"));
    }

    #[test]
    fn output_truncation() {
        let short = "hello";
        let (out, truncated) = combine_output(short, "", 1024);
        assert_eq!(out, short);
        assert!(!truncated);

        let long = "x".repeat(300);
        let (out, truncated) = combine_output(&long, "", 100);
        assert!(truncated);
        assert!(out.ends_with("[truncated]"));
        assert!(out.len() <= 100 + 20);
    }
}
