//! Builtin skillhub_cli tool implementation
//!
//! SkillHub 外置 CLI（用户已装的 Python 单文件脚本）的**薄壳封装**：
//! 向 LLM 工具面暴露 search/install/upgrade/list 四子命令，命令原样透传。
//!
//! # 设计口径（承方案 v2 与实施任务书）
//!
//! - **外置工件语义**：平台零打包零内嵌零复制，仅经进程调用引用用户已装 CLI
//! - **HOME 零改写**：子进程环境不覆盖 HOME（与 gh_cli per-user HOME 隔离相反）——
//!   CLI 自然复用用户 `~/.skillhub` 数据/配置目录；平台零写用户配置、零 env 注入
//! - **零 OTA 关断**：不追加跳过旗标、不注入跳过环境变量、不写 OTA 关断配置键
//!   （v1 三道保险零落地）——CLI 启动级自升级按 v2 放任口径如其自然发生，
//!   升级竞态以容错语义承载（如实报错+提示重试，不重试不回滚不干预）
//! - **路径探测四步降级**：config.command → `~/.skillhub/skillhub` → PATH →
//!   缺装引导（零安装命令零代装零 `curl | bash`）
//! - **白名单门**：仅放行 search/install/upgrade/list；login/publish/auth/config/
//!   self-upgrade 平台不开放（零凭证红线），校验在透传前（LLM 面防护门）
//! - **版本行回显**：每次调用后以 `--version` 追加探测附版本信息（探测失败
//!   降级 None 不阻断）；任意调用均可能伴随 CLI 启动级 OTA——回显如实呈现
//!   当下版本，零比对零劝升零降级提示
//! - **lockfile 只读观测**：install/upgrade 完成后只读探测
//!   `~/.skillhub/.skills_store_lock.json`（及无点前缀变体），读到则回显
//!   slug/version/source/sha256，未读到附 note 不阻断；只观测不管控
//! - **零凭证**：第一版不声明 credential_requirements（工厂默认空实现零覆盖）
//!
//! # 分层说明
//!
//! 纯 pkg 薄壳：进程调用经 `pkg::process::exec` 原语（零 shell 解释、零参数改写），
//! 白名单/路径探测/脱敏/截断为可注入纯函数（测试全函数级 mock，
//! 零真实安装零真实网络零真实 CLI 依赖）。

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::process::{ExecOptions, ExecOutput, exec};
use crate::pkg::request_context::RequestContext;
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// skillhub 二进制名（PATH 探测与默认安装布局文件名）
pub const SKILLHUB_CLI_BIN: &str = "skillhub";

/// 默认 CLI 路径规格（PO config 缺省值；`~` 按平台侧 HOME env 展开——仅探测用，
/// 子进程环境零改写）
pub const SKILLHUB_DEFAULT_PATH_SPEC: &str = "~/.skillhub/skillhub";

/// 子命令白名单（恰好四件；白名单外一律拒绝）
pub const ALLOWED_SUBCOMMANDS: &[&str] = &["search", "install", "upgrade", "list"];

/// 升级竞态容错引导语（如实报错+提示重试；不自动重试不回滚不干预）
pub const SELF_UPGRADE_RETRY_HINT: &str = "SkillHub CLI 可能正在自升级或暂时不可用，请稍后重试";

/// lockfile 未读到时的 note（不阻断不报错）
const LOCKFILE_NOTE: &str = "lockfile 未读到（可能无已装技能或路径变动）";

// ==================== 白名单门与路径探测（纯函数，依赖注入可测） ====================

/// 白名单门：空白切分 + 首词校验（透传前封装层校验，零 shell 解释）
///
/// 返回切分后的参数向量（exec 逐项传递，无空白切分歧义）；白名单外返回错误信息
///（列明四件白名单 + 不开放提示 + 零凭证红线）。
pub fn validate_subcommand(command: &str) -> std::result::Result<Vec<String>, String> {
    let args: Vec<String> = command.split_whitespace().map(str::to_string).collect();
    let Some(first) = args.first() else {
        return Err("command 不能为空".to_string());
    };
    if ALLOWED_SUBCOMMANDS.contains(&first.as_str()) {
        Ok(args)
    } else {
        Err(format!(
            "不支持的子命令 '{}'。skillhub_cli 仅开放: {}；\
             login/publish/auth/config/self-upgrade 平台不开放（零凭证红线）",
            first,
            ALLOWED_SUBCOMMANDS.join(", ")
        ))
    }
}

/// `~` 前缀路径规格展开（仅平台侧探测用；子进程环境零改写）
fn expand_home(spec: &str, home_dir: Option<&Path>) -> PathBuf {
    match spec.strip_prefix("~/") {
        Some(rest) => match home_dir {
            Some(home) => home.join(rest),
            None => PathBuf::from(spec),
        },
        None => PathBuf::from(spec),
    }
}

/// 路径探测四步降级（零代装）：
/// 1. config.command 显式指定（展开 `~` 后文件存在）
/// 2. `~/.skillhub/skillhub`（默认安装布局）
/// 3. PATH 扫描裸名 `skillhub`（如 `~/.local/bin` 布局，spawn 时由 OS 解析）
/// 4. 全部未命中 → 引导性错误（零安装命令零代装零 `curl | bash`）
///
/// `file_exists`/`path_lookup` 注入依赖（测试零真实文件系统/零真实 PATH）。
pub fn resolve_cli_path(
    config_command: Option<&str>,
    home_dir: Option<&Path>,
    file_exists: impl Fn(&Path) -> bool,
    path_lookup: impl Fn(&str) -> Option<PathBuf>,
) -> std::result::Result<String, String> {
    // 1. config.command 显式指定
    if let Some(spec) = config_command.map(str::trim).filter(|s| !s.is_empty()) {
        let p = expand_home(spec, home_dir);
        if file_exists(&p) {
            return Ok(p.to_string_lossy().to_string());
        }
    }
    // 2. 默认安装布局
    if let Some(home) = home_dir {
        let p = home.join(".skillhub").join(SKILLHUB_CLI_BIN);
        if file_exists(&p) {
            return Ok(p.to_string_lossy().to_string());
        }
    }
    // 3. PATH 扫描（裸名，spawn 时由 OS 解析）
    if let Some(found) = path_lookup(SKILLHUB_CLI_BIN) {
        return Ok(found.to_string_lossy().to_string());
    }
    // 4. 缺装引导
    Err(format!(
        "未找到 SkillHub CLI（{}）。请先安装 SkillHub CLI（skillhub.cn/install），\
         或在工具配置中修改命令路径",
        SKILLHUB_DEFAULT_PATH_SPEC
    ))
}

/// 生产探测接线：真实文件系统判定（含分隔符路径 is_file、裸名扫 PATH，
/// 与 tool_readiness::command_available 同语义）
fn production_file_exists(p: &Path) -> bool {
    if p.is_absolute() || p.components().count() > 1 {
        p.is_file()
    } else {
        crate::pkg::tool_registry::tool_readiness::command_available(&p.to_string_lossy())
    }
}

/// 生产探测接线：真实 PATH 扫描（命中返回裸名，spawn 时由 OS 解析）
fn production_path_lookup(name: &str) -> Option<PathBuf> {
    crate::pkg::tool_registry::tool_readiness::command_available(name).then(|| PathBuf::from(name))
}

// ==================== 输出脱敏与截断 ====================

/// 输出脱敏：token/api_key/secret/password/authorization 关键字行整行替换为
/// `[REDACTED]`（对齐 gh_cli sanitize 形态；skillhub 零凭证预期下的兜底防线）
pub fn sanitize_skillhub_output(output: &str) -> String {
    const SENSITIVE_KEYWORDS: &[&str] = &[
        "token",
        "authorization",
        "secret",
        "password",
        "api_key",
        "api-key",
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

/// UTF-8 安全截断：超限按字符边界收缩并追加截断标记
///（对齐 gh_cli 输出形态；直接字节切片在多字节字符边界会 panic，此处收缩至 char boundary）
fn truncate_utf8_safe(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_string(), false);
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}\n\n... [truncated]", &s[..end]), true)
}

// ==================== lockfile 只读观测 ====================

/// lockfile 只读候选（相对 `~/.skillhub/`；零硬编码唯一路径假设——十单探测时
/// 零技能在装、无 lockfile 产物，以多候选只读探测兜住布局漂移）
const LOCKFILE_CANDIDATES: &[&str] = &[".skills_store_lock.json", "skills_store_lock.json"];

/// 从 lockfile 条目提取观测键（slug/version/source/sha256，命中即回显）
fn extract_lock_entry(v: &Value) -> Option<Value> {
    let mut entry = serde_json::Map::new();
    for key in ["slug", "version", "source", "sha256"] {
        if let Some(val) = v.get(key) {
            entry.insert(key.to_string(), val.clone());
        }
    }
    (!entry.is_empty()).then_some(Value::Object(entry))
}

/// 解析 lockfile 内容：顶层对象 / 顶层数组 / 顶层容器键（skills/entries/packages）
/// 皆可；解析失败或零命中返回 None（不构成错误，只观测不管控）
fn parse_lockfile(content: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(content).ok()?;
    let entries: Vec<Value> = match &v {
        Value::Array(arr) => arr.iter().filter_map(extract_lock_entry).collect(),
        Value::Object(_) => match extract_lock_entry(&v) {
            Some(e) => vec![e],
            None => {
                let mut collected = Vec::new();
                for key in ["skills", "entries", "packages"] {
                    if let Some(Value::Array(arr)) = v.get(key) {
                        collected.extend(arr.iter().filter_map(extract_lock_entry));
                    }
                }
                collected
            }
        },
        _ => Vec::new(),
    };
    (!entries.is_empty()).then_some(Value::Array(entries))
}

/// install/upgrade 完成后的 lockfile 只读观测（读到回显四键/未读到 note 不阻断）
async fn observe_lockfile(home_dir: Option<&Path>) -> (Option<Value>, Option<String>) {
    let Some(home) = home_dir else {
        return (None, Some(LOCKFILE_NOTE.to_string()));
    };
    for name in LOCKFILE_CANDIDATES {
        let path = home.join(".skillhub").join(name);
        if let Ok(content) = tokio::fs::read_to_string(&path).await
            && let Some(observed) = parse_lockfile(&content)
        {
            return (Some(observed), None);
        }
    }
    (None, Some(LOCKFILE_NOTE.to_string()))
}

// ==================== 进程调用抽象（测试注入点） ====================

/// 文件存在性探测依赖（生产=is_file/PATH 语义；测试注入 mock）
type FileExistsFn = Arc<dyn Fn(&Path) -> bool + Send + Sync>;
/// PATH 扫描探测依赖（生产=扫 PATH 命中裸名；测试注入 mock）
type PathLookupFn = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

#[async_trait::async_trait]
trait CliRunner: Send + Sync {
    async fn run(&self, opts: &ExecOptions) -> Result<ExecOutput>;
}

/// 生产实现：转发 `pkg::process::exec` 原语（超时终止/管道并发读/零 shell 解释）
struct ExecRunner;

#[async_trait::async_trait]
impl CliRunner for ExecRunner {
    async fn run(&self, opts: &ExecOptions) -> Result<ExecOutput> {
        exec(opts).await
    }
}

// ==================== 工具定义 ====================

/// `CliToolConfig` 已下沉至 [`common::config::CliToolConfig`]（前后端共享 SSOT），
/// 此处仅转发兼容；skillhub 的缺省路径由 [`SKILLHUB_DEFAULT_PATH_SPEC`] 兜底。
pub use common::config::CliToolConfig;

/// `skillhub_cli` 工具参数
#[derive(Debug, Deserialize)]
pub struct SkillhubCliParams {
    /// 子命令与参数整串，如 `"search ima"`、`"install @org/slug"`、`"upgrade"`、`"list"`
    pub command: String,
    /// 超时（毫秒，覆盖默认 60s）
    pub timeout_ms: Option<u64>,
}

/// `skillhub_cli` 内置工具工厂（零凭证：不覆盖 BuiltinToolFactory 默认空声明）
#[derive(Debug, Clone, Default)]
pub struct SkillhubCliToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for SkillhubCliToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "skillhub_cli".to_string(),
            name: "SkillHub CLI".to_string(),
            description: concat!(
                "Operate the user's installed SkillHub CLI (external Python script) to manage ",
                "the SkillHub skill library: 'search' (find skills), 'install' (install a skill, ",
                "e.g. 'install @org/slug'), 'upgrade' (upgrade skills), 'list' (installed skills). ",
                "The command string is passed through to the CLI as-is. ",
                "login/publish/auth/config/self-upgrade are not exposed by this tool. ",
                "Prefer 'list'/'search' before 'install'. No credentials required."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Subcommand plus arguments passed through to the SkillHub CLI, e.g. 'search ima', 'install @org/slug', 'upgrade', 'list'. Only search/install/upgrade/list are allowed."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional: timeout in milliseconds. Default: 60000 (1 minute)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
            // CLI 命令进 PO config（D28：缺省默认路径规格，工具管理页可改命令路径）
            config: serde_json::json!(CliToolConfig {
                command: Some(SKILLHUB_DEFAULT_PATH_SPEC.to_string()),
                ..Default::default()
            }),
            // tag 双挂：`skillhub` 保留独立包（订阅式最小面），
            // `skill_management` 让「技能管理」工具包连同技能分发 CLI 一并唤醒注入
            // （AMan 2026-10-27 拍板：skillhub_cli 归入技能管理域）。
            // 必须写在代码侧：`sync_builtin_tools_to_db` 视 tags 为**代码所有权字段**
            // （每次启动 sync 以代码定义为准刷新 name/description/control_mode/
            //  parameters_schema/tags），运行时 update_tool 改 tags 会在下次启动被覆盖回去。
            tags: serde_json::to_string(&vec![
                "skillhub".to_string(),
                "skill_management".to_string(),
            ])
            .unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(SkillhubCliCoreTool::new(po))
    }
    // credential_requirements：零凭证第一版——沿用 trait 默认空实现（不覆盖）

    /// 就绪探测（生命周期方法①）：**语义 = 当前 `config.command` 是否可寻址**
    ///
    /// # 本项目根因的最终修复落点（2026-10-22 方案 A）
    ///
    /// 出厂默认 `SKILLHUB_DEFAULT_PATH_SPEC = "~/.skillhub/skillhub"` 在本机并不存在
    /// （CLI 实际装在 `~/.local/bin/skillhub`，属默认补全清单目录）。历史两个缺陷叠加：
    /// ①旧判定不展开 `~`（把 `~` 当普通字符 → 恒 not_ready）；
    /// ②判定侧走四级降级（含 PATH 兜底）而运行时判定无兜底 → 探针 `Ready`（让修复器
    /// 闸门「未就绪才修」跳过、配置永不纠正）、运行时 `NotReady`（前端长期未就绪）。
    ///
    /// 现在探针与运行时判定**同源同一函数**（`cli_binary_readiness` →
    /// `command_available`）：配置不可寻址必报 `NotReady` → 闸门放行 →
    /// [`Self::propose_self_heal`] 给出 `~/.local/bin/skillhub` → 配置被真正纠正。
    fn probe_readiness(&self, po: &ToolPo) -> common::api::RuntimeReady {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        crate::pkg::tool_registry::tool_readiness::cli_config_health_readiness(
            command.as_deref(),
            po.cli_install_hint().as_deref().unwrap_or_default(),
        )
    }

    /// 自愈配置提议（生命周期方法②）：只提议 `command` 单字段，不落库
    ///
    /// 真实案例:配置值 `~/.skillhub/skillhub` 不存在、CLI 实际在
    /// `~/.local/bin/skillhub` → 提议 `{"command": "<命中绝对路径>"}`。
    fn propose_self_heal(&self, po: &ToolPo) -> Option<serde_json::Value> {
        let command = po.cli_command().or_else(|| self.create_po().cli_command());
        crate::pkg::tool_registry::cli_lookup::cli_propose_heal(
            command.as_deref(),
            SKILLHUB_CLI_BIN,
            Some(SKILLHUB_DEFAULT_PATH_SPEC),
        )
    }
}

/// skillhub_cli 工具核心实现（薄壳：零协议知识零参数改写）
#[derive(Clone)]
pub struct SkillhubCliCoreTool {
    po: ToolPo,
    config: CliToolConfig,
    runner: Arc<dyn CliRunner>,
    /// 平台侧探测用 HOME（路径降级/lockfile 候选）；子进程环境零改写（HOME 不注入）
    home_dir: Option<PathBuf>,
    /// 文件存在性探测（生产=is_file/PATH 语义；测试注入 mock，零真实文件系统）
    file_exists: FileExistsFn,
    /// PATH 扫描探测（生产=扫 PATH；测试注入 mock，零真实 PATH）
    path_lookup: PathLookupFn,
}

impl SkillhubCliCoreTool {
    fn new(po: ToolPo) -> Self {
        Self::with_deps(
            po,
            Arc::new(ExecRunner),
            std::env::var_os("HOME").map(PathBuf::from),
            Arc::new(production_file_exists),
            Arc::new(production_path_lookup),
        )
    }

    /// 测试装配口：注入 runner 与探测依赖（零真实 CLI 零真实文件系统零真实 PATH）
    fn with_deps(
        po: ToolPo,
        runner: Arc<dyn CliRunner>,
        home_dir: Option<PathBuf>,
        file_exists: FileExistsFn,
        path_lookup: PathLookupFn,
    ) -> Self {
        let config = if po.config.is_null() {
            CliToolConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self {
            po,
            config,
            runner,
            home_dir,
            file_exists,
            path_lookup,
        }
    }

    /// `--version` 探测（同 program、零 env 注入；失败/超时/空输出 → None 不阻断；
    /// 任意调用均可能伴随 CLI 启动级 OTA——回显如实呈现当下版本，零比对零劝升）
    async fn probe_cli_version(&self, program: &str, timeout_ms: u64) -> Option<String> {
        let opts = ExecOptions::new(program, vec!["--version".to_string()])
            .timeout(Duration::from_millis(timeout_ms));
        let out = self.runner.run(&opts).await.ok()?;
        if !out.success {
            return None;
        }
        let mut line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if line.is_empty() {
            line = String::from_utf8_lossy(&out.stderr).trim().to_string();
        }
        let line = line.lines().next().unwrap_or("").trim().to_string();
        (!line.is_empty()).then_some(line)
    }
}

#[async_trait::async_trait]
impl CoreTool for SkillhubCliCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        let params: SkillhubCliParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let trimmed = params.command.trim();
        if trimmed.is_empty() {
            return Ok(json!({ "success": false, "error": "command 不能为空" }));
        }

        // 1. 白名单门（透传前封装层校验，LLM 面防护门，不依赖 CLI 侧行为）
        let cli_args = match validate_subcommand(trimmed) {
            Ok(a) => a,
            Err(msg) => return Ok(json!({ "success": false, "error": msg })),
        };

        // 2. 路径探测四步降级（零代装）
        let config_command = self.config.command(SKILLHUB_DEFAULT_PATH_SPEC);
        let program = match resolve_cli_path(
            Some(&config_command),
            self.home_dir.as_deref(),
            |p| (self.file_exists)(p),
            |n| (self.path_lookup)(n),
        ) {
            Ok(p) => p,
            Err(msg) => {
                return Ok(json!({
                    "success": false,
                    "error_code": "cli_not_installed",
                    "error": msg,
                    "install": "安装 SkillHub CLI：skillhub.cn/install",
                    "hint": "或确认 skillhub 已安装且在服务进程的 PATH 中，或在工具配置中修改命令路径"
                }));
            }
        };

        // 3. spawn（零 shell 解释零 env 注入——HOME 零改写；timeout 缺省 60s）
        let timeout_ms = params
            .timeout_ms
            .unwrap_or_else(|| self.config.default_timeout_ms());
        let opts =
            ExecOptions::new(&program, cli_args.clone()).timeout(Duration::from_millis(timeout_ms));
        let output = match self.runner.run(&opts).await {
            Ok(o) => o,
            Err(e) => {
                // spawn 失败（含 CLI re-exec 自替换窗口的瞬态 NotFound）→ 如实报错+重试引导
                return Ok(json!({
                    "success": false,
                    "error": format!("Failed to spawn skillhub CLI: {}", e),
                    "cli_version": Value::Null,
                    "hint": SELF_UPGRADE_RETRY_HINT,
                }));
            }
        };
        if output.timed_out {
            // exec 原语已在超时时终止子进程
            return Ok(json!({
                "success": false,
                "timeout": true,
                "timeout_ms": timeout_ms,
                "error": format!("skillhub CLI timed out after {} ms and was killed", timeout_ms),
                "cli_version": Value::Null
            }));
        }

        // 4. 非零退出 → 升级竞态容错：如实呈现 stderr 摘要+重试引导
        //   （不自动重试、不回滚、不干预、不吞错）
        if !output.success {
            let stderr = sanitize_skillhub_output(&String::from_utf8_lossy(&output.stderr));
            let stdout = sanitize_skillhub_output(&String::from_utf8_lossy(&output.stdout));
            let summary = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            let (summary, _) = truncate_utf8_safe(
                &summary,
                self.config.default_max_output_size_bytes() as usize,
            );
            // 升级窗口内 CLI 可能处于 re-exec 自替换中——版本探测照常 best-effort（失败降级 None）
            let cli_version = self.probe_cli_version(&program, timeout_ms).await;
            return Ok(json!({
                "success": false,
                "exit_code": output.exit_code,
                "error": summary,
                "cli_version": cli_version,
                "hint": SELF_UPGRADE_RETRY_HINT,
            }));
        }

        // 5. 输出组装（合并 stderr → 脱敏 → UTF-8 安全截断）
        let stdout_raw = String::from_utf8_lossy(&output.stdout);
        let stderr_raw = String::from_utf8_lossy(&output.stderr);
        let combined = if stderr_raw.trim().is_empty() {
            stdout_raw.to_string()
        } else {
            format!("{}\n[stderr]\n{}", stdout_raw, stderr_raw)
        };
        let sanitized = sanitize_skillhub_output(&combined);
        let (summary, truncated) = truncate_utf8_safe(
            &sanitized,
            self.config.default_max_output_size_bytes() as usize,
        );

        // 6. 版本行回显（--version 追加探测，失败降级 None 不阻断）
        let cli_version = self.probe_cli_version(&program, timeout_ms).await;

        let mut result = json!({
            "success": true,
            "exit_code": output.exit_code,
            "truncated": truncated,
            "output": summary,
            "cli_version": cli_version,
        });

        // 7. lockfile 只读观测（仅 install/upgrade；读到回显四键/未读到 note 不阻断）
        if matches!(
            cli_args.first().map(String::as_str),
            Some("install" | "upgrade")
        ) {
            let (observed, note) = observe_lockfile(self.home_dir.as_deref()).await;
            if let Some(observed) = observed {
                result["lockfile_observed"] = observed;
            }
            if let Some(note) = note {
                result["lockfile_note"] = json!(note);
            }
        }
        Ok(result)
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
    // credential_requirements：零凭证——CoreTool 默认空实现不覆盖
    // check：CoreTool 默认实现（注入非空即编排层错配防御）不覆盖
}

#[cfg(test)]
#[path = "skillhub_cli_tests.rs"]
mod tests;
