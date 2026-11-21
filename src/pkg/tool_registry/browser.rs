//! Builtin browser tool implementation（包装 agent-browser CLI）
//!
//! 通过 [agent-browser](https://github.com/vercel-labs/agent-browser) 让 Agent 获得
//! JS 渲染页阅读与页面交互能力，弥补 http_fetch 只能抓静态页的缺口。
//!
//! # 定位与边界（设计见 docs/modules/06-工具与技能）
//!
//! 本工具是 agent-browser CLI 的**配置管理层**：把需要人配的东西（命令路径、超时、
//! 启动环境变量、可选子命令屏蔽）做成配置页，而不是替用户决定「哪些 CLI 功能不许用」。
//! CLI 天然具备的能力一律开放——想绕限制的人走 shell_exec 一样能跑裸命令，硬禁只挡住
//! 老实人。默认不屏蔽任何子命令，管理员可在 config.disabled_commands 里拉黑明确有风险的
//! 命令（如 eval），不必枚举其余全部命令，CLI 升级新增命令也默认可用。
//!
//! - **能力开放**：所有 agent-browser 子命令默认可用（snapshot/eval/batch/tab/cookies/...）；
//!   仅管理员显式配置 disabled_commands 时才按黑名单屏蔽
//! - **会话隔离**：固定 `--session ai_orz_agent_{agent_id}`（非法字符折叠为 `_`），Agent 间互不干扰，
//!   daemon 常驻架构下页面状态跨调用保持
//! - **启动环境**：config.env 仅约束 `AGENT_BROWSER_` 前缀（防注入 PATH/LD_PRELOAD 劫持
//!   子进程），前缀内变量一律开放，风险仅在配置页提示
//! - **URL 入口**：open 只挡非 http(s) 与云元数据，本地/内网默认放行（测试场景开箱可用），
//!   仅字面检查不做 DNS 解析，完整边界靠浏览器级域名白名单 + 部署层隔离
//! - **截图产物**：screenshot 落统一产物存储（ScreenshotStorer 注入），返回 artifact 引用；
//!   Agent 可指定路径，未指定时用临时路径
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

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::http::validate_browser_open_url;
use crate::pkg::process::{ExecOptions, exec};
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::tool_readiness;
use crate::pkg::tool_registry::tool_security::fs::{
    FsAccessMode, ValidationResult, resolve_and_validate_path,
};
use common::config::BrowserConfig;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;

// 包装层默认开放 CLI 全部能力（含 eval/batch/tab/cookies 等）。管理员如需屏蔽高风险
// 命令，可在工具配置的 `disabled_commands` 里列黑名单；未配置时不拦截。

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
    /// 整串命令：如 `open https://example.com`、`fill @e3 "hello world"`、`skills get core`。
    /// 由 [`tokenize_command`] 做 shell 风格分词，首 token 即子命令（仅当管理员配置了
    /// disabled_commands 黑名单时才校验，默认不限制）。
    /// 不再拆 command/args 两个字段——与 gh_cli/lark_cli/skillhub_cli 的入参形态一致，
    /// Agent 读官方 skill 后去掉 `agent-browser` 前缀即可直接填入。
    pub command: String,
    /// 单次超时覆盖（毫秒，默认取工具 PO config 的 timeout_ms）
    pub timeout_ms: Option<u64>,
}

/// Shell 风格分词：空白切分 + 单/双引号包裹 + 反斜杠转义。
///
/// 为什么必须支持引号：`fill @e3 hello world` 的「hello world」是一个参数，
/// 朴素 `split_whitespace()` 会切碎成两个参数，导致填表内容错误。gh_cli 等
/// 用 split_whitespace 是因为其参数几乎无空格，browser 不行。
///
/// 不做命令替换 / 变量展开 / 管道 / 重定向——spawn 不经 shell，这里只是 argv 切分。
pub(crate) fn tokenize_command(raw: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = raw.chars().peekable();
    let mut quote: Option<char> = None;
    let mut started = false; // 当前 token 是否已开始（区分空引号 "" 与无 token）

    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' && q == '"' {
                    // 双引号内反斜杠转义下一个字符（" \ $ 等）
                    let next = chars
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("命令末尾的反斜杠没有后续字符"))?;
                    current.push(next);
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    started = true;
                }
                '\\' => {
                    let next = chars
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("命令末尾的反斜杠没有后续字符"))?;
                    current.push(next);
                    started = true;
                }
                c if c.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                c => {
                    current.push(c);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err(anyhow::anyhow!("命令引号未闭合").into());
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

/// 从 screenshot 子命令的 argv 里找出位置参数 PATH（用于产物归档）。
///
/// ⚠️ 不能简单取第一个非 `-` 开头的 token：带值选项的值也不带 `-`，例如
/// `screenshot --screen 0` 的 `0`、`--threshold 0.01` 的 `0.01`，会被误当 PATH，
/// 导致截图写到错文件、归档失败（desktop 官方文档示例正是 `screenshot --screen 0`）。
///
/// 规则：遍历 argv，`--` 之后的第一个 token 强制是位置参数（CLI 边界语义）；
/// 命中 `value_flags` 里的选项时跳过它和它的值（`--key=value` 值内嵌则不额外跳）；
/// 其余第一个非 `-` token 即 PATH。找不到返回 `None`（调用方注入临时路径）。
pub(crate) fn screenshot_path_arg(args: &[String], value_flags: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let token = &args[i];
        if token == "--" {
            // CLI 用 `--` 结束选项解析，其后第一个 token 是字面量位置参数
            return args.get(i + 1).cloned();
        }
        if token.starts_with('-') {
            // --key=value：值内嵌，不占后续 token
            if !token.contains('=') && value_flags.contains(&token.as_str()) {
                i += 2; // 跳过选项本身 + 它的值
            } else {
                i += 1;
            }
            continue;
        }
        return Some(token.clone());
    }
    None
}

/// agent-browser 二进制缺省命令（工具 PO config 的 `command` 缺省值来源，D28）
const DEFAULT_COMMAND: &str = "agent-browser";

/// 缺省单次超时（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// 缺省输出截断上限（字节）
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 262_144;

/// 安装引导（PO config.install_hint 缺省值来源；工厂默认与 spawn 引导共用，避免漂移）
const DEFAULT_INSTALL_HINT: &str = "brew install agent-browser 或 cargo install agent-browser（首次还需执行 agent-browser install 下载 Chrome）";

/// 命令路径修改引导（PO config 化后统一文案）
const CONFIG_PATH_HINT: &str = "或在工具配置中修改命令路径";

/// browser 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct BrowserToolFactory;

impl BuiltinToolFactory for BrowserToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "browser".to_string(),
            name: "Browser Automation".to_string(),
            description: concat!(
                "Drive a headless browser via the agent-browser CLI: read JS-rendered pages, ",
                "interact with web UIs, and test web apps. Pass ONE full command string (no ",
                "'agent-browser' prefix) — e.g. 'open https://example.com', 'snapshot -i', ",
                "'click @e3', 'fill @e3 \"hello world\"' (quote args containing spaces). ",
                "For the current command set and workflow, call 'skills get core' and ",
                "'skills get core --ref <name>' to read the official guide bundled with the ",
                "installed CLI; translate its commands by dropping the 'agent-browser' prefix. ",
                "Nearly all CLI subcommands work (snapshot/read/click/fill/screenshot/tab/batch/",
                "eval/network/cookies/etc.) — an admin may block specific risky ones via tool config. ",
                "Core loop: 'open <url>' then 'snapshot' for an accessibility tree with @eN refs; ",
                "act on refs, then re-snapshot after every page change because refs go stale. ",
                "To view a local HTML file, use 'open file:///absolute/path.html' — paths inside ",
                "the data directory work directly; outside paths return require_confirmation: stop ",
                "and ask the user for explicit confirmation before retrying. ",
                "'read' gives page text. Prefer 'snapshot'/'read' over 'screenshot' (visual proof ",
                "only); if an element is missing, 'wait' then snapshot again. Session state persists ",
                "across calls. Screenshots are archived as artifacts with {artifact_id, name, ",
                "local_path}. If output is truncated, the full text is saved to log_path — read ",
                "that file with fs_read to get everything (or raise config.max_output_bytes). ",
                "Prefer this tool over raw shell_exec: it adds per-agent session ",
                "isolation, entry-URL checks, and screenshot archiving for free."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            // Manual = 运行时经 request_tool_call 同步转发并立即返回，不是人工审批
            control_mode: ControlMode::Manual,
            parameters_schema: Some(json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Full agent-browser command WITHOUT the binary name, e.g. 'open https://example.com', 'snapshot -i', 'click @e3', 'fill @e3 \"hello world\"', 'skills get core'. Shell-style tokenized: quote args containing spaces. Use 'skills get core' to discover all supported commands."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: per-call timeout in milliseconds. Default: 60000 (from tool config)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            // CLI 命令与行为参数进 PO config（D28：工具自身属性，工具管理页可改）
            // 强类型约束：键集与 `common::config::BrowserConfig` 一一对应
            config: serde_json::to_value(BrowserConfig {
                command: Some(DEFAULT_COMMAND.to_string()),
                timeout_ms: Some(DEFAULT_TIMEOUT_MS),
                max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
                install_hint: Some(DEFAULT_INSTALL_HINT.to_string()),
                env: None,
                allow_local_network: None,
                disabled_commands: None,
                allow_file_url: None,
                allowed_file_dirs: None,
            })
            .unwrap_or_default(),
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

    /// 就绪探测（生命周期方法①）：**语义 = 当前 `config.command` 是否可寻址**
    ///
    /// 只回答「配置健康度」，**不做默认布局 / PATH 兜底** —— 兜底属于
    /// [`Self::propose_self_heal`]（怎么修）与执行侧 `resolve_cli_path`（怎么跑）。
    /// 实现直接复用运行时判定同一函数
    /// [`crate::pkg::tool_registry::tool_readiness::cli_config_health_readiness`]，
    /// 两处口径同源，不可能再分叉（2026-10-22 方案 A 定谳）。
    ///
    /// 组合语义：`NotReady` + 提议有解 = 「配置写错但可自愈」；
    /// `NotReady` + 提议 `None` = 「真的没装，交人工」。
    fn probe_readiness(&self, po: &ToolPo) -> common::api::RuntimeReady {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        crate::pkg::tool_registry::tool_readiness::cli_config_health_readiness(
            command.as_deref(),
            po.cli_install_hint().as_deref().unwrap_or_default(),
        )
    }

    /// 自愈配置提议（生命周期方法②）：只提议 `command` 单字段，不落库
    fn propose_self_heal(&self, po: &ToolPo) -> Option<serde_json::Value> {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        crate::pkg::tool_registry::cli_lookup::cli_propose_heal(
            command.as_deref(),
            DEFAULT_COMMAND,
            None,
        )
    }
}

/// browser 工具核心实现
#[derive(Debug, Clone)]
pub struct BrowserCoreTool {
    po: ToolPo,
}

/// 会话 ID：Agent 间隔离（daemon 常驻，页面状态跨调用保持）
///
/// CLI 的 session key 只接受 Unicode 字母/数字/`_`/`-`，且 `-` 在部分 CLI 参数解析里
/// 会被当成选项前缀。统一用 `_` 连接、非法字符折叠为 `_`，owner 退化为邮箱
/// （含 `@`/`.`）或 UUID（含 `-`）时也不会生成非法 key。owner 为空回落 `default`。
pub(crate) fn session_id(ctx: &RequestContext) -> String {
    let owner = ctx
        .agent_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| ctx.user_id.clone().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "default".to_string());
    let sanitized: String = owner
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if sanitized.chars().any(|c| c.is_alphanumeric()) {
        format!("ai_orz_agent_{sanitized}")
    } else {
        // owner 全是标点/符号（如 "!!!" / "???"）：折叠后全是 `_` 会串会话，
        // 追加确定性短哈希区分。正常 owner（UUID / 邮箱 / TEMPLATE_XXX）不走此分支。
        format!("ai_orz_agent_{sanitized}_{}", stable_suffix(&owner))
    }
}

/// 确定性 32 位 FNV-1a 哈希，输出 8 位 hex。用于退化 owner 的 session key 后缀，
/// 不依赖进程随机 seed（RandomState 每次启动不同会让同一 owner 的 key 漂移）。
fn stable_suffix(input: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in input.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// 输出合并：stderr 非空时附加 `[stderr]` 段
fn merge_output(stdout: &str, stderr: &str) -> String {
    if stderr.trim().is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n[stderr]\n{}", stdout, stderr)
    }
}

/// 输出截断：超限按 UTF-8 字符边界收缩并追加标记（直接字节切片在多字节字符处会 panic）。
/// 返回 (摘要, 是否截断, 全量字节数)；全量字节数用于回传 `full_output_bytes`。
pub(crate) fn combine_output(stdout: &str, stderr: &str, max_bytes: u64) -> (String, bool, usize) {
    let combined = merge_output(stdout, stderr);
    let full_bytes = combined.len();
    if full_bytes as u64 > max_bytes {
        let mut end = max_bytes as usize;
        while end > 0 && !combined.is_char_boundary(end) {
            end -= 1;
        }
        (
            format!("{}\n\n... [truncated]", &combined[..end]),
            true,
            full_bytes,
        )
    } else {
        (combined, false, full_bytes)
    }
}

/// 超预算输出全量落盘：写 `tools/browser/logs/{YYYYMMDD}/{call_id}.log`，
/// 与 shell_exec 同构，自动被既有日志 retention/cron 覆盖（tool_output_boundary_design 红线 1：
/// 禁止静默截断，回传引用的底气是①已全量留盘）。
///
/// base 由调用方从 config 注入（此处不读全局单例，便于单测用 tmp 目录且不依赖初始化）。
async fn persist_full_output(
    base: &std::path::Path,
    ctx: &RequestContext,
    full: &str,
) -> Result<std::path::PathBuf> {
    let call_id = ctx
        .tool_call_id
        .clone()
        .unwrap_or_else(|| ctx.log_id.clone());
    let day = chrono::Local::now().format("%Y%m%d").to_string();
    let dir = crate::pkg::paths::tool_logs_dir(base, "browser").join(day);
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{}.log", call_id));
    tokio::fs::write(&path, full).await?;
    Ok(path)
}

#[async_trait]
impl CoreTool for BrowserCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. 参数解析 + shell 风格分词（首 token 即子命令）
        // command 收整串（如 `fill @e3 "hello world"`），与 gh_cli/lark_cli 入参形态一致；
        // 分词失败（引号未闭合等）返回结构化错误，不 spawn。
        let params: BrowserParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let tokens = match tokenize_command(params.command.trim()) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => {
                return Ok(json!({
                    "success": false,
                    "error": "command 不能为空。示例：open https://example.com、snapshot -i、skills get core"
                }));
            }
            Err(e) => {
                return Ok(json!({
                    "success": false,
                    "error": format!("command 分词失败：{e}（引号/反斜杠未闭合？）")
                }));
            }
        };
        let raw_command = tokens[0].clone();
        let subcommand = raw_command.to_lowercase();
        let mut cli_args: Vec<String> = tokens.into_iter().skip(1).collect();

        // 2. CLI 就绪预检 + 配置解析（强类型 `BrowserConfig`）
        // 配置走强类型 `BrowserConfig`（键与 `ToolPo::cli_command` 等访问器一致；
        // 存量 config=Null → 结构体缺省兜底，零迁移，D28）
        let mut config: BrowserConfig = if self.po.config.is_null() {
            BrowserConfig::default()
        } else {
            match serde_json::from_value::<BrowserConfig>(self.po.config.clone()) {
                Ok(config) => config,
                Err(e) => {
                    // 强类型解析整包失败 ⇒ 全部字段回落内置缺省（含 command）：
                    // 不留痕时「配置被改坏后仍跑默认命令」会变成排查黑洞
                    crate::log_warn!(
                        ctx,
                        "browser",
                        "browser config 反序列化失败，回退内置缺省（command / timeout / max_output）: {}",
                        e
                    );
                    BrowserConfig::default()
                }
            }
        };

        // 可选子命令黑名单：默认不屏蔽（开放 CLI 全部能力）；只有管理员显式在配置页
        // 填了 disabled_commands 才屏蔽命中项。这是「配置管理而非硬编码封禁」的体现。
        if let Some(blocked) = config.disabled_commands.as_ref()
            && !blocked.is_empty()
            && blocked.iter().any(|c| c.eq_ignore_ascii_case(&subcommand))
        {
            return Ok(json!({
                "success": false,
                "error": format!(
                    "子命令 '{}' 已被本工具配置禁用。已屏蔽：{}。如需恢复请在工具配置页的「禁用的子命令」中移除。",
                    raw_command,
                    blocked.join("/")
                )
            }));
        }

        // screenshot：路径由调用方决定。给了位置参数（路径）就用它；没给才注入临时路径
        // 并落产物中心。两种情况都在成功后附产物引用——不限制 Agent 写-path 的能力。
        // 用 screenshot_path_arg 跳过带值选项（--threshold 0.01 等），避免把数值当 PATH。
        let mut screenshot_path: Option<PathBuf> = None;
        let mut screenshot_uses_temp = false;
        if subcommand == "screenshot" {
            let value_flags = [
                "--threshold",
                "--screenshot-dir",
                "--screenshot-format",
                "--screenshot-quality",
                "--hide-scrollbars",
            ];
            if let Some(explicit) = screenshot_path_arg(&cli_args, &value_flags) {
                screenshot_path = Some(PathBuf::from(explicit));
            } else {
                let path = std::env::temp_dir().join(format!(
                    "ai-orz-browser-shot-{}-{}.png",
                    chrono::Utc::now().timestamp_millis(),
                    ctx.tool_call_id
                        .clone()
                        .unwrap_or_else(|| "anon".to_string())
                ));
                cli_args.push(path.to_string_lossy().to_string());
                screenshot_path = Some(path);
                screenshot_uses_temp = true;
            }
        }
        // 运行时兜底校验：config 正常经 update_tool 的单点校验写入，但绕过页面
        // 直改 DB 可能塞入 PATH/LD_PRELOAD 等键；exec 是纯叠加无过滤，故此处
        // 复用同一校验，非法则整项丢弃并留痕（不影响其它合法配置字段）。
        if let Some(env) = config.env.as_ref()
            && let Ok(value) = serde_json::to_value(env)
            && let Err(e) = common::models::tool::validate_browser_env(&value)
        {
            crate::log_warn!(
                ctx,
                "browser",
                "config.env 含非法键，已忽略全部启动环境变量: {}",
                e
            );
            config.env = None;
        }
        // open URL 入口检查：
        // - http(s)：SSRF 字面检查，只挡非 http(s)/云元数据；本地/内网默认放行，
        //   仅管理员显式配 allow_local_network=false 才收紧。完整边界靠浏览器级域名白名单。
        // - file://：与 fs_read/fs_write 完全对齐——复用同一套 resolve_and_validate_path
        //   （canonicalize + starts_with + 敏感名 + 符号链接），作用域=数据根+allowed_file_dirs；
        //   作用域外返回 require_confirmation 让 Agent 停下来问用户授权，不是硬拒。
        //   allow_file_url 默认放行，仅管理员显式配 false 才完全禁用。Agent 参数无法覆盖。
        if subcommand == "open"
            && let Some(url_arg) = cli_args.iter().find(|arg| !arg.starts_with('-'))
        {
            let is_file = reqwest::Url::parse(url_arg)
                .map(|u| u.scheme() == "file")
                .unwrap_or(false);
            if is_file {
                if !config.allow_file_url.unwrap_or(true) {
                    return Ok(json!({
                        "success": false,
                        "error": "本地文件访问已被本工具配置禁用（allow_file_url=false）",
                        "hint": "如需恢复请在工具配置页开启 allow_file_url"
                    }));
                }
                let url = reqwest::Url::parse(url_arg)
                    .map_err(|e| anyhow::anyhow!("URL 格式无效：{e}"))?;
                let path = url.to_file_path().map_err(|_| {
                    common::error::Error::from(anyhow::anyhow!(
                        "file:// 路径无效：仅支持本机绝对路径，不接受 file://host/path 远程形态"
                    ))
                })?;
                let base = match crate::config::try_get() {
                    Some(cfg) => cfg.base_data_path(),
                    None => {
                        return Ok(json!({
                            "success": false,
                            "error": "无法校验本地文件路径：服务配置未初始化"
                        }));
                    }
                };
                let additional: &[String] = config.allowed_file_dirs.as_deref().unwrap_or(&[]);
                match resolve_and_validate_path(
                    &base,
                    &path.to_string_lossy(),
                    additional,
                    FsAccessMode::Read,
                )? {
                    ValidationResult::Valid(_) => {}
                    ValidationResult::NeedConfirmation(message) => {
                        return Ok(json!({
                            "success": false,
                            "require_confirmation": true,
                            "message": message
                        }));
                    }
                }
            } else if let Err(e) =
                validate_browser_open_url(url_arg, config.allow_local_network.unwrap_or(true))
            {
                return Ok(json!({
                    "success": false,
                    "error": format!("URL 被安全策略拒绝：{e}"),
                    "hint": "此检查仅覆盖入口 URL；完整边界靠浏览器级域名白名单与网络隔离"
                }));
            }
        }

        let bin = config
            .command
            .clone()
            .unwrap_or_else(|| DEFAULT_COMMAND.to_string());
        let install_hint = config
            .install_hint
            .clone()
            .unwrap_or_else(|| DEFAULT_INSTALL_HINT.to_string());
        if !tool_readiness::command_available(&bin) {
            return Ok(tool_readiness::cli_not_installed_json(
                "agent-browser",
                &install_hint,
                CONFIG_PATH_HINT,
            ));
        }

        // 3. spawn（不经 shell，argv 直拼；--session 隔离）
        // timeout / max_output 同源读 `BrowserConfig`（params.timeout_ms 优先，缺省常量兜底）
        let session = session_id(&ctx);
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| config.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        let max_output_bytes = config.max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);

        // 启动环境变量：config.env 经 common 单点校验（仅 AGENT_BROWSER_ 前缀，
        // 防 PATH/LD_PRELOAD 劫持），此处按 key 排序保证 exec 参数稳定（HashMap 无序）。
        // 语义为「在父进程环境上叠加」：未列出的继承，列出的覆盖（exec.rs 语义）。
        let mut launch_env: Vec<(String, String)> =
            config.env.clone().unwrap_or_default().into_iter().collect();
        launch_env.sort_by(|a, b| a.0.cmp(&b.0));

        let output = match exec(
            &ExecOptions::new(
                &bin,
                [
                    vec!["--session".to_string(), session.clone(), subcommand.clone()],
                    cli_args,
                ]
                .concat(),
            )
            .envs(launch_env)
            .timeout(Duration::from_millis(timeout_ms)),
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                // spawn 错误分类：NotFound → 安装引导；PermissionDenied → 权限提示
                return Ok(match e.code {
                    common::error::ErrorCode::NotFound => tool_readiness::cli_not_installed_json(
                        "agent-browser",
                        &install_hint,
                        CONFIG_PATH_HINT,
                    ),
                    common::error::ErrorCode::Forbidden => json!({
                        "success": false,
                        "error": format!("agent-browser 无执行权限，请 chmod +x 或检查路径: {}", e)
                    }),
                    _ => json!({
                        "success": false,
                        "error": format!("启动 agent-browser 失败: {}", e)
                    }),
                });
            }
        };

        // 4. 超时保护 + 输出合并截断（超时由 exec 原语终止子进程）
        if output.timed_out {
            return Ok(json!({
                "success": false,
                "error": format!("agent-browser 执行超时（{}ms），已终止子进程", timeout_ms),
                "hint": "页面加载过慢时可分步执行：先 open 再 wait，或调大 timeout_ms 参数"
            }));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let (combined, truncated, full_output_bytes) =
            combine_output(&stdout, &stderr, max_output_bytes);

        let mut payload = json!({
            "success": output.success,
            "command": subcommand,
            "session": session,
            "exit_code": output.exit_code,
            "output": combined,
            "truncated": truncated,
            "full_output_bytes": full_output_bytes
        });

        // 超预算截断必须非静默（tool_output_boundary_design 红线 1）：全量落盘 + 回传引用，
        // Agent 需要全文时用 fs_read 自助取回，框架不代劳。落盘失败不否定命令本身的结果，
        // 但如实告知「全文未留存」，避免 Agent 误以为还能取回。
        if truncated {
            let full = merge_output(&stdout, &stderr);
            let persisted = match crate::config::try_get() {
                None => {
                    crate::log_warn!(ctx, "browser", "browser 全量输出落盘跳过：配置单例未初始化");
                    Err(())
                }
                Some(cfg) => {
                    let base = cfg.base_data_path();
                    persist_full_output(&base, &ctx, &full).await.map_err(|e| {
                        crate::log_warn!(ctx, "browser", "browser 全量输出落盘失败: {}", e);
                    })
                }
            };
            match persisted {
                Ok(path) => {
                    let shown = path.to_string_lossy().into_owned();
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert(
                            "output".to_string(),
                            json!(format!("{}\nfull output saved to: {}", combined, shown)),
                        );
                    }
                    payload["log_path"] = json!(shown);
                    payload["hint"] = json!(format!(
                        "输出超过 {} 字节已截断；完整输出（{} 字节）已落盘，用 fs_read 读取该文件取全文。",
                        max_output_bytes, full_output_bytes
                    ));
                }
                Err(_) => {
                    payload["hint"] = json!(format!(
                        "输出超过 {} 字节已截断，且全量落盘失败；请调大 config.max_output_bytes 后重试。",
                        max_output_bytes
                    ));
                }
            }
        }

        // 5. screenshot 分支：产物落统一存储，返回引用（不内嵌 base64）
        if subcommand == "screenshot" && output.success {
            let path = screenshot_path.expect("screenshot path set above");
            payload["screenshot"] = match store_screenshot(ctx.clone(), &path).await {
                Ok(artifact) => {
                    // 只清理系统生成的临时文件；Agent 指定的路径是它的产物，保留
                    if screenshot_uses_temp {
                        let _ = tokio::fs::remove_file(&path).await;
                    }
                    json!({
                        "stored": true,
                        "artifact_id": artifact.artifact_id,
                        "name": artifact.name,
                        "local_path": path.to_string_lossy().to_string()
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
#[path = "browser_tests.rs"]
mod tests;
