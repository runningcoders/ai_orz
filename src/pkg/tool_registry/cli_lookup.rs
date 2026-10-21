//! CLI 命令发现与解析 —— 平台侧「命令查找 + 就绪校验」单点真源
//!
//! # 为什么需要这个模块（本项目 01a15c47 的根因）
//!
//! 内置工具 `skillhub_cli` 的默认 `config.command` 写死为 `~/.skillhub/skillhub`，
//! 而用户机器实际把 CLI 装在 `~/.local/bin/skillhub`。旧的就绪判定
//! （`tool_readiness::command_available`）**不展开 `~`**：
//! `Path::new("~/.skillhub/skillhub")` 里 `~` 只是普通字符，`components().count() > 1`
//! 成立 → 直接 `is_file()` 判那个不存在的路径 → **恒定 false** → 工具长期显示
//! 「未就绪」。与此同时执行侧 `skillhub_cli.rs` 自己有一套四步降级
//! `resolve_cli_path`（会展开 `~`、会扫 PATH），所以**调用其实能跑通**。
//!
//! 判定侧与执行侧两套逻辑分叉，就是「未就绪但实际可用」的精确机制。本模块把
//! 「怎么找命令」收敛为一处：判定侧（`probe_readiness`）与执行侧（`resolve_cli_path`）
//! 共用同一解析函数，从根本上消除分叉。
//!
//! # 查找策略（顺序即优先级）
//!
//! 1. **显式配置优先**：`config.command` 若含分隔符或 `~` → 展开后判文件存在；
//! 2. **默认布局兜底**：工具声明的默认安装路径（如 `~/.skillhub/skillhub`）；
//! 3. **PATH 发现**：裸名扫「父进程 PATH + 内置补全目录
//!    （`~/.cargo/bin`、`~/.local/bin`、`~/.nvm/versions/node/*/bin` 等）」；
//! 4. **缺装引导**：全部未命中 → 引导性错误（**零代装、零 `curl | bash`**）。
//!
//! # 红线
//!
//! - **不改 `PATH` 环境变量**：补全目录只在查找时叠加，不写回进程环境；
//! - **不代装**：任何情况下不下载、不执行安装脚本；
//! - **不实跑 `--version`**：本模块只做文件级判定（skillhub 类 CLI 有启动级 OTA，
//!   实跑属显式授权事项，永不进自动链路）。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use common::api::RuntimeReady;

/// 平台侧 HOME（`HOME` 优先，Windows 回退 `USERPROFILE`）
pub fn current_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// `~` 前缀展开：`~/xxx` → `home/xxx`；无 `~` 前缀原样返回
///
/// 仅展开 `~/`（与 shell 语义一致）；`~user/` 不做展开（平台不假设多用户布局）。
pub fn expand_home(spec: &str, home_dir: Option<&Path>) -> PathBuf {
    match spec.strip_prefix("~/") {
        Some(rest) => match home_dir {
            Some(home) => home.join(rest),
            None => PathBuf::from(spec),
        },
        None => PathBuf::from(spec),
    }
}

/// 单个路径条目展开：`~` 前缀 + **单层** `*` 通配
///
/// `~/.nvm/versions/node/*/bin` 这类条目需要通配；只支持 `*`（前缀匹配），
/// 不支持 `**`/`?`/字符集——够用即可，不做 glob 引擎。
pub fn expand_entry(entry: &str) -> Vec<PathBuf> {
    let expanded = expand_home(entry, current_home().as_deref());
    let text = expanded.to_string_lossy().into_owned();
    if !text.contains('*') {
        return vec![expanded];
    }

    let mut cursor: Vec<PathBuf> = if text.starts_with('/') {
        vec![PathBuf::from("/")]
    } else {
        vec![PathBuf::new()]
    };
    for segment in text.split('/').filter(|s| !s.is_empty()) {
        let mut next: Vec<PathBuf> = Vec::new();
        if segment.contains('*') {
            let prefix = segment.trim_end_matches('*');
            for base in &cursor {
                let Ok(entries) = std::fs::read_dir(base) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(prefix) {
                        next.push(base.join(name));
                    }
                }
            }
        } else {
            for base in &cursor {
                next.push(base.join(segment));
            }
        }
        cursor = next;
        if cursor.is_empty() {
            break;
        }
    }
    cursor
}

/// 查找目录序列：父进程 PATH 在前，内置补全目录在后（去重、只收真实目录）
///
/// **只读**：不修改进程 `PATH`。
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    for addition in crate::pkg::tool_registry::shell_env::default_path_additions() {
        for dir in expand_entry(&addition) {
            if dir.is_dir() && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// 裸名在「PATH + 补全目录」中查找，命中返回**绝对路径**（spawn 时无需再解析）
pub fn lookup_in_search_path(name: &str) -> Option<PathBuf> {
    for dir in search_dirs() {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 命令可寻址统一口径（就绪判定与执行路径解析共用）
///
/// - 含分隔符 / `~` 前缀 → 展开 `~` 后判 `is_file()`（**这一步就是 `~` 展开缺失的修复点**）
/// - 纯裸名 → 扫「PATH + 内置补全目录」
pub fn command_available(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let expanded = expand_home(trimmed, current_home().as_deref());
    if expanded.is_absolute() || expanded.components().count() > 1 {
        return expanded.is_file();
    }
    lookup_in_search_path(trimmed).is_some()
}

/// 四级降级解析（依赖注入版；测试零真实文件系统零真实 PATH）
///
/// 顺序：`config` 显式 → `fallback_spec` 默认布局 → PATH 裸名 → 缺装（`Err`）。
/// `Err` 只承载「全部未命中」这一事实，**文案由调用方组装**（各工具引导语不同）。
pub fn resolve_cli_command_with(
    config_command: Option<&str>,
    home_dir: Option<&Path>,
    fallback_spec: Option<&str>,
    bin_name: &str,
    file_exists: impl Fn(&Path) -> bool,
    path_lookup: impl Fn(&str) -> Option<PathBuf>,
) -> std::result::Result<String, ResolveMiss> {
    // 1. config 显式指定
    if let Some(spec) = config_command.map(str::trim).filter(|s| !s.is_empty()) {
        let expanded = expand_home(spec, home_dir);
        if file_exists(&expanded) {
            return Ok(expanded.to_string_lossy().into_owned());
        }
        // 显式值本身是裸名（单段）→ 仍然允许 PATH 兜底（配置写「skillhub」也算有效声明）
        if !expanded.is_absolute()
            && expanded.components().count() <= 1
            && let Some(found) = path_lookup(spec)
        {
            return Ok(found.to_string_lossy().into_owned());
        }
    }
    // 2. 工具声明的默认安装布局
    if let Some(spec) = fallback_spec.map(str::trim).filter(|s| !s.is_empty()) {
        let expanded = expand_home(spec, home_dir);
        if file_exists(&expanded) {
            return Ok(expanded.to_string_lossy().into_owned());
        }
    }
    // 3. PATH 扫裸名
    if let Some(found) = path_lookup(bin_name) {
        return Ok(found.to_string_lossy().into_owned());
    }
    Err(ResolveMiss {
        bin_name: bin_name.to_string(),
    })
}

/// 解析全部未命中（只携带事实，文案归调用方）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveMiss {
    /// 被查找的二进制名
    pub bin_name: String,
}

/// 生产版四级降级解析（真实文件系统 + 真实 PATH）
pub fn resolve_cli_command(
    config_command: Option<&str>,
    bin_name: &str,
    fallback_spec: Option<&str>,
) -> std::result::Result<String, ResolveMiss> {
    let home = current_home();
    resolve_cli_command_with(
        config_command,
        home.as_deref(),
        fallback_spec,
        bin_name,
        |p| p.is_file(),
        lookup_in_search_path,
    )
}

// ==================== 供 BuiltinToolFactory 默认实现复用的就绪探测/自愈提议 ====================

/// 就绪探测：命令可解析 → `Ready`；否则 `NotReady{cli_not_installed}`
///
/// 与 `tool_readiness::cli_binary_readiness` 同一输出形态（reason/hint 语义一致），
/// 只是判定口径改走 `resolve_cli_command`（含 `~` 展开与 PATH 补全兜底）。
pub fn cli_probe_readiness(
    command: Option<&str>,
    bin_name: &str,
    fallback_spec: Option<&str>,
    install_hint: &str,
) -> RuntimeReady {
    match resolve_cli_command(command, bin_name, fallback_spec) {
        Ok(_) => RuntimeReady::Ready,
        Err(_) => {
            let config_hint = "或在工具配置中修改命令路径";
            let hint = if install_hint.trim().is_empty() {
                config_hint.to_string()
            } else {
                format!("{}；{}", install_hint.trim(), config_hint)
            };
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint,
            }
        }
    }
}

/// 自愈提议：当前命令不可解析、但能在默认布局或 PATH 里找到 → 提议新 `command`
///
/// 只提议、不落库；返回的 JSON **只含 `command` 一项**（平台侧仍会按 allowlist
/// 二次提取，多余字段丢弃并 warn）。全无解（真的没装）→ `None`，交人工。
pub fn cli_propose_heal(
    command: Option<&str>,
    bin_name: &str,
    fallback_spec: Option<&str>,
) -> Option<Value> {
    let current = command.map(str::trim).filter(|s| !s.is_empty())?;
    // 已经可用 → 无需自愈（幂等前置）
    if command_available(current) {
        return None;
    }
    // 默认安装布局可用？
    if let Some(spec) = fallback_spec.map(str::trim).filter(|s| !s.is_empty()) {
        let candidate = expand_home(spec, current_home().as_deref());
        if candidate.is_file() {
            return Some(json!({ "command": candidate.to_string_lossy() }));
        }
    }
    // PATH 命中？
    if let Some(found) = lookup_in_search_path(bin_name) {
        return Some(json!({ "command": found.to_string_lossy() }));
    }
    None
}

// ==================== 单元测试 ====================
#[cfg(test)]
#[path = "cli_lookup_tests.rs"]
mod tests;
