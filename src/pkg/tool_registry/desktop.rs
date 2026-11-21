//! Builtin desktop tool implementation（包装 agent-desktop CLI）
//!
//! 通过 [agent-desktop](https://github.com/vercel-labs/agent-desktop)（Vercel Labs，
//! Apache-2.0，Rust 实现 + C FFI）让 Agent 像人一样操作桌面原生应用：读 OS 无障碍树、
//! 对 ref 点击/输入、再 snapshot 验证。与 `browser` 工具同属 agent-browser.dev 家族，
//! 命令风格 / `skills get` 自描述 / JSON envelope / snapshot-ref 循环完全一致，故包装壳
//! 也完全同构——本工具是 CLI 的**配置管理层**，不是能力封禁系统。
//!
//! # 定位与边界
//!
//! - **能力开放**：所有 agent-desktop 子命令默认可用；仅管理员显式配 `disabled_commands`
//!   黑名单时才屏蔽命中项（想绕的人走 shell_exec 一样能跑裸命令，硬禁只挡老实人）
//! - **会话隔离**：`--session ai_orz_agent_{agent_id}`（非法字符折叠为 `_`），Agent 间快照/ref 命名空间互不干扰
//! - **headless 默认**：CLI 默认走 a11y API 不抢光标/焦点；需物理输入时 Agent 自行加 `--headed`
//! - **截图产物**：`screenshot [PATH]` 未给路径时注入临时路径并落统一产物中心，返回引用
//! - **spawn 不经 shell**：argv 直拼，无注入面；输出超时 + UTF-8 安全截断 + 全量落盘
//!
//! # 分层说明
//!
//! 截图产物存储器 trait 定义在 pkg 层（browser 模块，无上层依赖），由 project Domain
//! 实现并在 `service::init` 注册；本工具与 browser 共用同一已注册实例，不直连 DAL/DAO。
//! 分词 / 输出截断为纯算法，直接复用 browser 模块的 crate 可见实现，不重复造轮子。

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::process::{ExecOptions, exec};
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::browser::{
    ScreenshotArtifact, combine_output, get_screenshot_storer, screenshot_path_arg, session_id,
    tokenize_command,
};
use crate::pkg::tool_registry::tool_readiness;
use common::config::DesktopConfig;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;

/// agent-desktop 二进制缺省命令（PO config 的 `command` 缺省值来源，D28）
const DEFAULT_COMMAND: &str = "agent-desktop";

/// 缺省单次超时（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// 缺省输出截断上限（字节）
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 262_144;

/// 安装引导（macOS 需授予辅助功能 / 屏幕录制权限）
const DEFAULT_INSTALL_HINT: &str = "npm install -g agent-desktop（macOS 还需给启动进程授予「辅助功能」与「屏幕录制」权限，可用 agent-desktop permissions --guide 引导）";

/// 命令路径修改引导
const CONFIG_PATH_HINT: &str = "或在工具配置中修改命令路径";

/// `desktop` 工具参数（与 browser 同构：整串 command 透传）
#[derive(Debug, Deserialize)]
pub struct DesktopParams {
    /// 整串命令（不含 `agent-desktop` 前缀），如 `list-apps`、`snapshot --app Finder -i`、
    /// `click @ref`、`skills get desktop`。由 [`tokenize_command`] 做 shell 风格分词。
    pub command: String,
    /// 单次超时覆盖（毫秒，默认取工具 PO config 的 timeout_ms）
    pub timeout_ms: Option<u64>,
}

/// desktop 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct DesktopToolFactory;

impl BuiltinToolFactory for DesktopToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "desktop".to_string(),
            name: "Desktop Automation".to_string(),
            description: concat!(
                "See and operate native desktop apps (Finder, VS Code, Office, settings, any GUI ",
                "app) via the agent-desktop CLI — reads the OS accessibility tree, no screenshots ",
                "or pixel matching needed. Pass ONE full command string WITHOUT the 'agent-desktop' ",
                "prefix — e.g. 'list-apps', 'snapshot --app Finder -i', 'click @ref', ",
                "'type @ref \"hello world\"' (quote args containing spaces). Before first use call ",
                "'skills get desktop' and 'skills get platform' to read the official guides bundled ",
                "with the installed CLI; translate their commands by dropping the 'agent-desktop' ",
                "prefix. Core loop: 'list-apps' to find the app, 'snapshot --app NAME -i' for an ",
                "accessibility tree with @ref ids, act with 'click'/'type'/'set-value', then ",
                "snapshot again to verify. Refs belong to their snapshot and go stale when the UI ",
                "changes (STALE_REF: take a fresh snapshot). Headless by default — actions use the ",
                "accessibility API without moving the pointer or stealing focus; add '--headed' only ",
                "when physical input is required. 'screenshot [PATH]' captures a PNG (needs Screen ",
                "Recording permission on macOS); without a path a temp file is used and archived as ",
                "an artifact {artifact_id, name, local_path}. If output is truncated, the full text ",
                "is saved to log_path — read that file with fs_read to get everything. Prefer this ",
                "tool over raw shell_exec: it adds per-agent session isolation and screenshot ",
                "archiving for free. For web-only tasks prefer the browser tool (structured DOM)."
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
                        "description": "Full agent-desktop command WITHOUT the binary name, e.g. 'list-apps', 'snapshot --app Finder -i', 'click @s8f3k2p9:e8', 'skills get desktop'. Shell-style tokenized: quote args containing spaces. Use 'skills get desktop' to discover all supported commands."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: per-call timeout in milliseconds. Default: 60000 (from tool config)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            config: serde_json::to_value(DesktopConfig {
                command: Some(DEFAULT_COMMAND.to_string()),
                timeout_ms: Some(DEFAULT_TIMEOUT_MS),
                max_output_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
                install_hint: Some(DEFAULT_INSTALL_HINT.to_string()),
                disabled_commands: None,
            })
            .unwrap_or_default(),
            // 首 tag=browser → 前端与 browser 归入「browser」组（同家族一组工具包）；
            // 次 tag=desktop 保留语义区分，将来拆组只改首 tag 一处。
            tags: serde_json::to_string(&vec![
                "browser".to_string(),
                "desktop".to_string(),
            ])
            .unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(DesktopCoreTool { po })
    }

    /// 就绪探测：语义 = 当前 `config.command` 是否可寻址（与 browser 同源同函数）
    fn probe_readiness(&self, po: &ToolPo) -> common::api::RuntimeReady {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        tool_readiness::cli_config_health_readiness(
            command.as_deref(),
            po.cli_install_hint().as_deref().unwrap_or_default(),
        )
    }

    /// 自愈配置提议：只提议 `command` 单字段，不落库
    fn propose_self_heal(&self, po: &ToolPo) -> Option<serde_json::Value> {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        crate::pkg::tool_registry::cli_lookup::cli_propose_heal(
            command.as_deref(),
            DEFAULT_COMMAND,
            None,
        )
    }
}

/// desktop 工具核心实现
#[derive(Debug, Clone)]
pub struct DesktopCoreTool {
    po: ToolPo,
}

/// 超预算输出全量落盘：`tools/desktop/logs/{YYYYMMDD}/{call_id}.log`，与 browser 同构
async fn persist_full_output(
    base: &std::path::Path,
    ctx: &RequestContext,
    full: &str,
) -> Result<PathBuf> {
    let call_id = ctx
        .tool_call_id
        .clone()
        .unwrap_or_else(|| ctx.log_id.clone());
    let day = chrono::Local::now().format("%Y%m%d").to_string();
    let dir = crate::pkg::paths::tool_logs_dir(base, "desktop").join(day);
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{}.log", call_id));
    tokio::fs::write(&path, full).await?;
    Ok(path)
}

#[async_trait]
impl CoreTool for DesktopCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. 参数解析 + shell 风格分词
        let params: DesktopParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let tokens = match tokenize_command(params.command.trim()) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => {
                return Ok(json!({
                    "success": false,
                    "error": "command 不能为空。示例：list-apps、snapshot --app Finder -i、skills get desktop"
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

        // 2. 强类型 config（DesktopConfig；存量 config=Null → 缺省兜底，零迁移，D28）
        let config: DesktopConfig = if self.po.config.is_null() {
            DesktopConfig::default()
        } else {
            match serde_json::from_value::<DesktopConfig>(self.po.config.clone()) {
                Ok(config) => config,
                Err(e) => {
                    crate::log_warn!(
                        ctx,
                        "desktop",
                        "desktop config 反序列化失败，回退内置缺省（command / timeout / max_output）: {}",
                        e
                    );
                    DesktopConfig::default()
                }
            }
        };

        // 可选子命令黑名单：默认不屏蔽；管理员显式配置才拦截
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

        // screenshot：位置参数 PATH 决定输出文件；未给则注入临时路径并落产物中心。
        // 用 screenshot_path_arg 跳过带值选项——官方示例 `screenshot --screen 0 shot.png`
        // 里 `0` 是 --screen 的值，不能被误当 PATH。
        let mut screenshot_path: Option<PathBuf> = None;
        let mut screenshot_uses_temp = false;
        if subcommand == "screenshot" {
            let value_flags = ["--app", "--window-id", "--screen"];
            if let Some(explicit) = screenshot_path_arg(&cli_args, &value_flags) {
                screenshot_path = Some(PathBuf::from(explicit));
            } else {
                let path = std::env::temp_dir().join(format!(
                    "ai-orz-desktop-shot-{}-{}.png",
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
                "agent-desktop",
                &install_hint,
                CONFIG_PATH_HINT,
            ));
        }

        // 3. spawn（不经 shell，argv 直拼；--session 隔离，与 browser 同款命名）
        let session = session_id(&ctx);
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| config.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        let max_output_bytes = config.max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);

        let output = match exec(
            &ExecOptions::new(
                &bin,
                [
                    vec!["--session".to_string(), session.clone(), subcommand.clone()],
                    cli_args,
                ]
                .concat(),
            )
            .timeout(Duration::from_millis(timeout_ms)),
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                return Ok(match e.code {
                    common::error::ErrorCode::NotFound => tool_readiness::cli_not_installed_json(
                        "agent-desktop",
                        &install_hint,
                        CONFIG_PATH_HINT,
                    ),
                    common::error::ErrorCode::Forbidden => json!({
                        "success": false,
                        "error": format!("agent-desktop 无执行权限，请 chmod +x 或检查路径: {}", e)
                    }),
                    _ => json!({
                        "success": false,
                        "error": format!("启动 agent-desktop 失败: {}", e)
                    }),
                });
            }
        };

        // 4. 超时保护 + 输出合并截断
        if output.timed_out {
            return Ok(json!({
                "success": false,
                "error": format!("agent-desktop 执行超时（{}ms），已终止子进程", timeout_ms),
                "hint": "复杂操作可先 snapshot 再分步执行，或调大 timeout_ms 参数"
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

        // 超预算截断必须非静默：全量落盘 + 回传引用，Agent 用 fs_read 自助取回
        if truncated {
            let full = format!(
                "{}{}",
                stdout,
                if stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!("\n[stderr]\n{}", stderr)
                }
            );
            let persisted = match crate::config::try_get() {
                None => {
                    crate::log_warn!(ctx, "desktop", "desktop 全量输出落盘跳过：配置单例未初始化");
                    Err(())
                }
                Some(cfg) => {
                    let base = cfg.base_data_path();
                    persist_full_output(&base, &ctx, &full).await.map_err(|e| {
                        crate::log_warn!(ctx, "desktop", "desktop 全量输出落盘失败: {}", e);
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

        // 5. screenshot 分支：产物落统一存储（复用 browser 注册的存储器），返回引用
        if subcommand == "screenshot" && output.success {
            let path = screenshot_path.expect("screenshot path set above");
            payload["screenshot"] = match store_screenshot(ctx.clone(), &path).await {
                Ok(artifact) => {
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

/// 截图落产物存储（复用 browser 模块注册的全局存储器）
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
#[path = "desktop_tests.rs"]
mod tests;
