//! Shell 子进程环境变量统一出口
//!
//! 三条会起子进程的链路（`shell_exec` / 声明式 shell 工具 / MCP stdio）此前各写各的
//! 环境处理：`shell_exec` 有一份白名单过滤 + HOME 改写，`shell_tool` 什么都不做，
//! MCP 走 `env_clear()` 零继承。本模块把「该给子进程哪些环境变量」收口到一处。
//!
//! # 为什么需要 PATH 补全
//!
//! 子进程继承的是**服务进程**的环境。服务进程常由 IDE / launchd / systemd 拉起，
//! PATH 往往只有 `/usr/bin:/bin:/usr/sbin:/sbin`；且 `shell_exec` 用 `/bin/sh -c`
//! 非交互执行（不读 `~/.zshrc` / `~/.bash_profile`），nvm / pyenv / cargo 这类靠
//! rc 文件注入 PATH 的版本管理器全部失效——表现为 `command not found`。
//!
//! # 契约
//!
//! - **只产出增量**：返回的 map 由调用方决定怎么施加——继承式（`command.env` 叠加，
//!   父进程环境仍在）或零继承式（`env_clear()` 后注入）
//! - **PATH 补全恒追加到尾部**：不覆盖既有解析顺序，显式配置优先于系统默认
//! - **身份变量恒注入**：`AI_ORZ_TASK_ID` / `AI_ORZ_AGENT_ID` 供 git commit-msg
//!   hook 等原生扩展点读取（见 `pkg/git_workspace.rs`）
//!
//! # 环境变量策略：兼容优先（不做白名单）
//!
//! 子进程**继承服务进程全部环境变量**，这是有意选择而非遗漏：
//!
//! - 命令本身已受两道约束：`shell_policy` 拦截层（路径 scope + 危险命令规则）
//!   + `ControlMode::Manual` 每次调用都要用户批准
//! - 真白名单（先 `env_clear()` 再放行）的代价远大于收益：`TMPDIR` 缺失会让
//!   clang/node 等一大票 CLI 在 macOS 上直接失败，`LANG` / `DYLD_*` / `XDG_*` /
//!   `SSL_CERT_FILE` 同理，且这类故障极难归因
//! - 因此 `ShellExecConfig.allowed_env` **不是安全边界**，只是「额外显式带上」的
//!   变量清单；不要在任何文档/描述里把它描述成隔离机制
//! - 推论：不要把服务端凭据放进服务进程环境（用配置文件），否则子进程可读；
//!   唯一真正零继承的是 MCP stdio 链路（凭据隔离红线，见 `mcp.rs`）

use crate::pkg::tool_registry::shell_policy;
use common::config::HomeMode;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 敏感变量子串：被 `allowed_env` 显式放行时也会被剔除
///
/// 仅作用于「显式注入集合」，**不构成安全边界**——继承式链路的父进程环境仍在，
/// 子进程照样能读到。见模块文档「环境变量策略：兼容优先」。
const SENSITIVE_SUBSTRINGS: &[&str] = &[
    "home",
    "user",
    "username",
    "password",
    "token",
    "secret",
    "api_key",
    "aws_access_key_id",
    "aws_secret_access_key",
    "google_application_credentials",
    "ssh_auth_sock",
    "git_config",
    "git_ssh",
];

/// 构造子进程环境变量的请求（全部可选，按需填）
#[derive(Debug, Default, Clone)]
pub struct ShellEnvRequest<'a> {
    /// 显式带出的变量（取值来自服务进程环境；**非白名单**，其余变量仍会被继承）
    pub allowed_env: &'a [String],
    /// PATH 补全目录（None = 内置默认 `common::config::ShellConfig`）
    /// shell_exec 从 ToolPo.config 取；声明式 shell 工具 / MCP 用内置默认
    pub path_additions: Option<&'a [String]>,
    /// 调用方（模型 / 工具配置）额外指定的环境变量，优先级最高
    pub extra_env: Option<&'a HashMap<String, String>>,
    /// 任务 ID → `AI_ORZ_TASK_ID`
    pub task_id: Option<&'a str>,
    /// Agent ID → `AI_ORZ_AGENT_ID`
    pub agent_id: Option<&'a str>,
}

/// 解析子进程需要显式注入的环境变量
///
/// 顺序：白名单过滤 → 合并 extra → PATH 补全 → 注入身份变量。
/// HOME 不在本函数职责内（各链路策略不同：`shell_exec` 改写为隔离 HOME，
/// 声明式 shell 工具继承父进程 HOME）。
pub fn resolve(request: &ShellEnvRequest<'_>) -> HashMap<String, String> {
    let mut env = filter_inherited_environment(request.allowed_env);
    if let Some(extra) = request.extra_env {
        for (key, value) in extra {
            env.insert(key.clone(), value.clone());
        }
    }
    let additions = match request.path_additions {
        Some(additions) => additions.to_vec(),
        None => default_path_additions(),
    };
    if let Some(path) = completed_path(env.get("PATH").map(String::as_str), &additions) {
        env.insert("PATH".to_string(), path);
    }
    // 拦截层固定出口步骤：结构性注入任务/Agent 身份（不可绕），
    // 供 git commit-msg hook 等原生扩展点读取
    if let Some(task_id) = request.task_id {
        env.insert(shell_policy::ENV_TASK_ID.to_string(), task_id.to_string());
    }
    if let Some(agent_id) = request.agent_id {
        env.insert(shell_policy::ENV_AGENT_ID.to_string(), agent_id.to_string());
    }
    env
}

/// 取出需要显式注入的变量（敏感子串恒定剔除）
///
/// 名字里的 `filter` 仅指「从父进程环境里挑出这些键」，**不是安全过滤**：
/// 调用方若不 `env_clear()`，父进程环境仍会完整继承给子进程。
pub fn filter_inherited_environment(allowed: &[String]) -> HashMap<String, String> {
    std::env::vars()
        .filter(|(key, _)| {
            if !allowed.contains(key) {
                return false;
            }
            let key_lower = key.to_lowercase();
            !SENSITIVE_SUBSTRINGS.iter().any(|s| key_lower.contains(s))
        })
        .collect()
}

/// 合并额外环境变量到基础集合（原 `merge_extra_environment`，Value 版保留给 JSON 入参）
pub fn merge_extra_environment(
    base: HashMap<String, String>,
    extra: &Value,
) -> HashMap<String, String> {
    let mut base = base;
    if let Some(obj) = extra.as_object() {
        for (key, value) in obj {
            if let Some(val_str) = value.as_str() {
                base.insert(key.clone(), val_str.to_string());
            }
        }
    }
    base
}

/// 内置默认的 PATH 补全目录（工具级未配置时使用）
///
/// 注意：这不是配置文件项——工具行为类配置放 ToolPo.config，见
/// `common::config::ShellConfig` 的文档注释。
pub fn default_path_additions() -> Vec<String> {
    common::config::ShellConfig::default().path_additions
}

/// 补全面的 PATH 值
///
/// `base` 为起点（None = 取父进程 PATH）；仅追加存在且尚未包含、且**是目录**的条目。
/// 返回 None 表示连父进程 PATH 都没有（此时不应凭空造一个）。
pub fn completed_path(base: Option<&str>, additions: &[String]) -> Option<String> {
    let current = match base {
        Some(value) => value.to_string(),
        None => std::env::var("PATH").ok()?,
    };
    let mut entries: Vec<PathBuf> = std::env::split_paths(&current).collect();
    for addition in additions {
        for dir in expand_entry(addition) {
            if dir.is_dir() && !entries.contains(&dir) {
                entries.push(dir);
            }
        }
    }
    std::env::join_paths(entries)
        .ok()
        .map(|joined| joined.to_string_lossy().into_owned())
}

/// 子进程 HOME 取值
///
/// - `Isolated`：用户隔离 HOME（`{base}/users/{user_id}`），git/gh 复用该用户配置
/// - `Inherit`：返回 `None` —— 调用方不要注入 HOME，子进程自然继承服务进程 HOME
///
/// 无 user_id 时无法定位隔离 HOME，同样返回 `None`（保持继承）。
pub fn home_for(mode: HomeMode, user_id: Option<&str>, base_root: &Path) -> Option<PathBuf> {
    match mode {
        HomeMode::Inherit => None,
        HomeMode::Isolated => user_id.map(|uid| crate::pkg::paths::user_home(base_root, uid)),
    }
}

/// 零继承链路（MCP `env_clear()` 之后）与声明式 shell 工具用的 PATH 值
///
/// 只给 PATH：零继承红线针对的是凭据泄漏，PATH 不含凭据；但没有 PATH 时
/// `npx` / `uvx` 这类 server 内部再起子进程必然失败。
pub fn inherited_path() -> Option<String> {
    completed_path(None, &default_path_additions())
}

/// 隔离 HOME 下的工具链根目录变量注入
///
/// `home_mode = isolated` 把子进程 HOME 指向隔离目录，git/gh 身份隔离了，
/// 但 cargo / nvm / pyenv 这类把配置装在真实 `$HOME` 下的工具链随之失效。
/// 这批工具链都支持官方环境变量指定根目录，于是可兼得：HOME 走隔离目录，
/// `CARGO_HOME` / `NVM_DIR` 等指回真实 HOME。
///
/// `names` 来自 ToolPo.config `toolchain_envs`（工具链名，大小写不敏感）：
/// 未知名忽略；对应路径在真实 HOME 下**不存在则跳过**（对齐 PATH 补全的
/// 「存在才追加」语义，不给子进程指空目录）。调用方应以 `or_insert` 施加，
/// 让 `params.env` 显式传的同名变量保持最高优先级。
pub fn toolchain_env_injections(names: &[String]) -> Vec<(String, String)> {
    toolchain_env_injections_with(names, home_dir().as_deref())
}

/// 可注入版本的内部实现（`real_home` 显式传入便于测试）
fn toolchain_env_injections_with(
    names: &[String],
    real_home: Option<&Path>,
) -> Vec<(String, String)> {
    let Some(real_home) = real_home else {
        return Vec::new();
    };
    let mut injections = Vec::new();
    for name in names {
        let lowered = name.trim().to_lowercase();
        let Some((_, env_var, relative)) = common::models::tool::SHELL_TOOLCHAIN_HOME_VARS
            .iter()
            .find(|(supported, _, _)| *supported == lowered)
        else {
            continue;
        };
        let target = real_home.join(relative);
        if target.exists() {
            injections.push((
                (*env_var).to_string(),
                target.to_string_lossy().into_owned(),
            ));
        }
    }
    injections
}

/// 隔离 HOME 下的 `GIT_SSH_COMMAND` 注入（git over ssh 的 known_hosts 补齐）
///
/// isolated HOME 没有 `.ssh` 目录，git 走 `git@...` 远端时的问题分层：
/// - key：`SSH_AUTH_SOCK` 是继承式链路本就携带的变量，agent 里的 key 照常可用
/// - known_hosts：缺失会让非交互 ssh 卡在严格主机校验上直接失败——这是
///   本函数补的缺口。known_hosts 是公开的主机指纹，不敏感
/// - 私钥文件 / `~/.ssh/config`：**绝不注入**（不做 `-i`、不 symlink `.ssh`），
///   身份本体不进 Agent 任务的命令视野
///
/// 注入条件（三者缺一不可，全部满足才产出）：
/// 1. 服务进程有 `SSH_AUTH_SOCK`（没有 agent，key 无从谈起）
/// 2. 真实 HOME 下 `~/.ssh/known_hosts` 存在（对齐「存在才注入」语义）
/// 3. 调用方在 isolated 且未走 `env.HOME` 逃生舱（由 `shell_exec` 保证）
///
/// 调用方应以 `or_insert` 施加，`params.env` 显式传的同名变量保持最高优先级。
pub fn git_ssh_command_injection() -> Option<String> {
    git_ssh_command_injection_with(
        std::env::var_os("SSH_AUTH_SOCK").is_some(),
        home_dir().as_deref(),
    )
}

/// 可注入版本的内部实现（agent 存在性与真实 HOME 显式传入便于测试）
fn git_ssh_command_injection_with(has_agent: bool, real_home: Option<&Path>) -> Option<String> {
    if !has_agent {
        return None;
    }
    let known_hosts = real_home?.join(".ssh").join("known_hosts");
    if !known_hosts.is_file() {
        return None;
    }
    Some(format!(
        "ssh -o UserKnownHostsFile={} -o StrictHostKeyChecking=accept-new",
        shell_single_quoted(&known_hosts.to_string_lossy())
    ))
}

/// shell 单引号包裹（GIT_SSH_COMMAND 由 shell 再解析，路径含空格/引号需转义）
fn shell_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// 展开单条目录配置：`~` 前缀 + 单段 `*` 通配
fn expand_entry(entry: &str) -> Vec<PathBuf> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let path = match trimmed.strip_prefix("~/") {
        Some(rest) => match home_dir() {
            Some(home) => home.join(rest),
            None => return Vec::new(),
        },
        None if trimmed == "~" => match home_dir() {
            Some(home) => home,
            None => return Vec::new(),
        },
        None => PathBuf::from(trimmed),
    };
    expand_wildcard(&path)
}

/// 递归展开路径中的 `*` 通配段（每段取修改时间最新的一个匹配目录，
/// 用于 `~/.nvm/versions/node/*/bin` 这类带版本号的目录）
fn expand_wildcard(path: &Path) -> Vec<PathBuf> {
    let Some(position) = path
        .components()
        .position(|component| component.as_os_str() == "*")
    else {
        return vec![path.to_path_buf()];
    };
    let mut parent = PathBuf::new();
    let mut rest = PathBuf::new();
    for (index, component) in path.components().enumerate() {
        if index < position {
            parent.push(component);
        } else if index > position {
            rest.push(component);
        }
    }
    if parent.as_os_str().is_empty() {
        parent = PathBuf::from(".");
    }
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return Vec::new();
    };
    let mut dirs: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| candidate.is_dir())
        .map(|candidate| (modified_time(&candidate), candidate))
        .collect();
    dirs.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    dirs.into_iter()
        .take(1)
        .flat_map(|(_, candidate)| expand_wildcard(&candidate.join(&rest)))
        .collect()
}

fn modified_time(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|meta| meta.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "shell_env_tests.rs"]
mod tests;
