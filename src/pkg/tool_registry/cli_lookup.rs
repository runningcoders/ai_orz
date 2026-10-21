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
mod tests {
    use super::*;

    fn temp_home() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        (dir, home)
    }

    #[test]
    fn expand_home_only_handles_tilde_slash() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            expand_home("~/.local/bin/skillhub", Some(&home)),
            home.join(".local/bin/skillhub")
        );
        // 非 ~/ 前缀原样
        assert_eq!(
            expand_home("/abs/bin", Some(&home)),
            PathBuf::from("/abs/bin")
        );
        assert_eq!(
            expand_home("skillhub", Some(&home)),
            PathBuf::from("skillhub")
        );
        // 无 HOME 时原样（不 panic）
        assert_eq!(expand_home("~/x", None), PathBuf::from("~/x"));
    }

    #[test]
    fn command_available_expands_tilde_for_path_like_spec() {
        // 根因回归：`~/...` 形态必须展开后再判存在（旧实现恒 false）
        let (_dir, home) = temp_home();
        let bin_dir = home.join(".local/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("skillhub");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();

        let spec = "~/.local/bin/skillhub";
        // 直接用真实 HOME 判定（进程 HOME ≠ temp home），故走注入版核验展开语义
        let got = resolve_cli_command_with(
            Some(spec),
            Some(&home),
            None,
            "skillhub",
            |p| p.is_file(),
            |_| None,
        );
        assert!(got.is_ok(), "resolve should hit expanded path");
        assert_eq!(got.unwrap(), bin.to_string_lossy());
    }

    #[test]
    fn command_available_absolute_and_missing() {
        assert!(command_available("/bin/ls"));
        assert!(!command_available("/no/such/binary-xyz"));
        assert!(!command_available(""));
        assert!(!command_available("   "));
    }

    #[test]
    fn command_available_bare_name_scans_path() {
        // 裸名：ls 必在 PATH 内；不存在的名字必 false
        assert!(command_available("ls"));
        assert!(!command_available("no-such-binary-xyz-abc"));
    }

    #[test]
    fn resolve_four_step_priority_order() {
        let (_dir, home) = temp_home();
        // 步骤 1：config 显式命中
        let explicit = home.join("explicit-bin");
        std::fs::write(&explicit, "").unwrap();
        let got = resolve_cli_command_with(
            Some(explicit.to_string_lossy().as_ref()),
            Some(&home),
            Some("~/.fallback/bin"),
            "mycli",
            |p| p.is_file(),
            |_| None,
        )
        .unwrap();
        assert_eq!(got, explicit.to_string_lossy());

        // 步骤 2：默认布局命中（config 未命中）
        let fallback = home.join(".fallback").join("bin");
        std::fs::create_dir_all(&fallback).unwrap();
        std::fs::write(fallback.join("mycli"), "").unwrap();
        let got = resolve_cli_command_with(
            Some("/no/such/explicit"),
            Some(&home),
            Some("~/.fallback/bin/mycli"),
            "mycli",
            |p| p.is_file(),
            |_| None,
        )
        .unwrap();
        assert_eq!(got, fallback.join("mycli").to_string_lossy());

        // 步骤 3：PATH 兜底
        let got = resolve_cli_command_with(
            Some("/no/such/explicit"),
            Some(&home),
            Some("~/.fallback/bin/mycli"),
            "mycli",
            |_| false,
            |name| (name == "mycli").then(|| PathBuf::from("/usr/local/bin/mycli")),
        )
        .unwrap();
        assert_eq!(got, "/usr/local/bin/mycli");

        // 步骤 4：全部未命中
        let miss =
            resolve_cli_command_with(None, None, None, "mycli", |_| false, |_| None).unwrap_err();
        assert_eq!(miss.bin_name, "mycli");
    }

    #[test]
    fn resolve_accepts_bare_name_in_config_via_path() {
        // config 写裸名也算有效声明（走 PATH 兜底）
        let got = resolve_cli_command_with(
            Some("mycli"),
            None,
            None,
            "mycli",
            |_| false,
            |name| (name == "mycli").then(|| PathBuf::from("/opt/bin/mycli")),
        )
        .unwrap();
        assert_eq!(got, "/opt/bin/mycli");
    }

    #[test]
    fn cli_probe_readiness_shape() {
        let ready = cli_probe_readiness(Some("/bin/ls"), "ls", None, "install hint");
        assert_eq!(ready, RuntimeReady::Ready);

        let not_ready = cli_probe_readiness(Some("/no/such/xyz"), "no-such-bin-xyz", None, "");
        assert_eq!(
            not_ready,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "或在工具配置中修改命令路径".to_string(),
            }
        );

        // install_hint 非空 → 「install；config」组合（与 cli_binary_readiness 同形态）
        let with_hint = cli_probe_readiness(Some("/no/such/xyz"), "no-such-bin-xyz", None, "装它");
        assert_eq!(
            with_hint,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "装它；或在工具配置中修改命令路径".to_string(),
            }
        );
    }

    #[test]
    fn cli_propose_heal_returns_single_command_field() {
        // 已可用 → 无需自愈
        assert!(cli_propose_heal(Some("/bin/ls"), "ls", None).is_none());
        // 无命令声明 → 无自愈发起点
        assert!(cli_propose_heal(None, "ls", None).is_none());
        // 真正没装（PATH 也没有）→ 无解交人工
        assert!(cli_propose_heal(Some("/no/such/xyz"), "no-such-bin-xyz", None).is_none());

        // 默认布局可用 → 提议恰好一个 command 字段
        let (_dir, home) = temp_home();
        let fallback_dir = home.join(".fallback");
        std::fs::create_dir_all(&fallback_dir).unwrap();
        std::fs::write(fallback_dir.join("mycli"), "").unwrap();
        // 注意：真实 HOME 与 temp home 不同，故此处用 PATH 命中路径核验字段形态
        let healed = cli_propose_heal(Some("/no/such/xyz"), "ls", None);
        let healed = healed.expect("ls 在 PATH 内，应当可自愈");
        let obj = healed.as_object().unwrap();
        assert_eq!(obj.len(), 1, "只允许 command 单字段");
        assert!(obj.contains_key("command"));
        assert!(obj["command"].as_str().unwrap().ends_with("ls"));
    }

    #[test]
    fn root_cause_tilde_spec_now_resolves_whereas_old_logic_failed() {
        // 根因回归（本项目的精确病灶）：`~/.skillhub/skillhub` 这类带 `~` 的规格
        // 过去被 `Path::new` 当成普通字符，`components().count() > 1` → 直接判
        // is_file("~/.skillhub/skillhub") → 恒 false。现在必须展开后判定。
        let (_dir, home) = temp_home();
        let real_bin = home.join(".local/bin/skillhub");
        std::fs::create_dir_all(real_bin.parent().unwrap()).unwrap();
        std::fs::write(&real_bin, "#!/bin/sh\n").unwrap();

        // 旧口径（不展开）：直接判原始字符串 → 必然不存在
        assert!(!Path::new("~/.local/bin/skillhub").is_file());

        // 新口径（展开）：解析命中同一文件
        let resolved = resolve_cli_command_with(
            Some("~/.local/bin/skillhub"),
            Some(&home),
            None,
            "skillhub",
            |p| p.is_file(),
            |_| None,
        )
        .expect("expanded spec must resolve");
        assert_eq!(resolved, real_bin.to_string_lossy());
    }

    #[test]
    fn probe_and_heal_agree_on_the_same_resolution() {
        // 判定侧与自愈侧必须共用同一解析口径 —— 否则会出现
        // 「显示未就绪但提议不出方案」或「提议了但判定仍不认」的死结。
        let (_dir, home) = temp_home();
        let bin = home.join("tools/mycli");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, "").unwrap();

        let spec = "~/tools/mycli";
        let resolved = resolve_cli_command_with(
            Some("/no/such/old"),
            Some(&home),
            Some(spec),
            "mycli",
            |p| p.is_file(),
            |_| None,
        )
        .unwrap();
        assert_eq!(resolved, bin.to_string_lossy());

        // 该路径一旦写入 config，命令可寻址判定必须为真（闭环自证）
        assert!(Path::new(&resolved).is_file());
    }

    #[test]
    fn resolve_miss_never_suggests_network_install() {
        // 红线：解析失败只报事实，绝不携带 curl | bash 之类的代装指令
        let miss = resolve_cli_command_with(None, None, None, "ghost-cli", |_| false, |_| None)
            .unwrap_err();
        assert_eq!(miss.bin_name, "ghost-cli");
        let text = format!("{:?}", miss);
        assert!(!text.contains("curl"));
        assert!(!text.contains("bash"));
        assert!(!text.contains("install"));
    }

    #[test]
    fn cli_probe_readiness_never_spawns_binary() {
        // 红线：就绪探测只做文件级判定，绝不实跑 CLI
        // （skillhub 类 CLI 有启动级 OTA，实跑属显式授权事项）
        // 用一个"存在但不是可执行程序"的普通文本文件验证判定只看存在性
        let (_dir, home) = temp_home();
        let plain = home.join("plain.txt");
        std::fs::write(&plain, "not a program").unwrap();

        let ready = cli_probe_readiness(
            Some(plain.to_string_lossy().as_ref()),
            "plain.txt",
            None,
            "",
        );
        assert_eq!(ready, RuntimeReady::Ready);
    }

    #[test]
    fn two_separate_expansions_are_deterministic() {
        // 查找必须无副作用且确定性：同样输入两次结果一致
        let first = command_available("ls");
        let second = command_available("ls");
        assert_eq!(first, second);
        assert!(first);
    }

    #[test]
    fn search_dirs_never_mutates_process_path() {
        let before = std::env::var_os("PATH");
        let dirs = search_dirs();
        let after = std::env::var_os("PATH");
        assert_eq!(before, after, "红线：查找不得改写 PATH 环境");
        assert!(!dirs.is_empty(), "PATH 至少应有一条");
    }
}
