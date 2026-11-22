//! browser 工具配置页的「动态探测」：从 agent-browser 自带 skills 文档解析环境变量目录，
//! 结合本机浏览器 / 代理探测给出推荐值，再与 common 的中文精选兜底合并为 DTO。
//!
//! 红线：探测只服务配置引导，不参与就绪判定；任何一步失败都返回 `None`
//! （前端回落静态目录），绝不阻塞详情页。所有子进程调用带短超时，结果带 TTL 缓存。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// 注：HashMap 仅用于 build_recommendations 的返回值，缓存本身为单条（见 CACHE）。

use common::api::{BrowserEnvEntryDTO, BrowserEnvOptionDTO, BrowserEnvProbeDTO};
use common::models::tool::{BROWSER_ENV_CATALOG, BrowserEnvKind, browser_env_blocked_reason};

use crate::pkg::process::exec::{ExecOptions, exec};

/// 探测结果缓存窗口：详情页会因时间筛选反复重拉，不能每次都 spawn 子进程
const TTL: Duration = Duration::from_secs(60);
const DOC_TIMEOUT: Duration = Duration::from_secs(3);
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);
/// 文档输出上限（防异常输出撑爆内存；CLI 实际约 40KB）
const MAX_DOC_BYTES: usize = 256 * 1024;

/// 单条缓存：探测结果只依赖「CLI 自带文档 + 本机浏览器 / 代理环境」，二者都与
/// 配置里的 `command` 字符串无关（command 变了不会让文档或本机浏览器变）。因此
/// 用按 command 索引的 HashMap 只会让管理员每次改路径都新增一个永不过期的条目，
/// 是无界增长；单条 + TTL 即可，换 command 时探测结果仍然有效。
static CACHE: OnceLock<Mutex<Option<(Instant, BrowserEnvProbeDTO)>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<(Instant, BrowserEnvProbeDTO)>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

fn cached() -> Option<BrowserEnvProbeDTO> {
    let guard = cache().lock().ok()?;
    let (at, dto) = guard.as_ref()?;
    (at.elapsed() < TTL).then(|| dto.clone())
}

fn store(dto: BrowserEnvProbeDTO) {
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((Instant::now(), dto));
    }
}

/// 详情页探测入口：全部失败路径返回 `None`，调用方保持 DTO 字段为空即可。
pub async fn probe(command: Option<&str>) -> Option<BrowserEnvProbeDTO> {
    if let Some(dto) = cached() {
        return Some(dto);
    }
    let bin = command
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("agent-browser");
    let dto = probe_uncached(bin).await?;
    store(dto.clone());
    Some(dto)
}

async fn probe_uncached(bin: &str) -> Option<BrowserEnvProbeDTO> {
    // CLI 自带 skills 文档与版本严格匹配，是唯一需要动态跟进的「环境变量清单」来源
    let doc = exec(
        &ExecOptions::new(
            bin,
            ["skills", "get", "core", "--ref", "commands"]
                .iter()
                .map(|arg| (*arg).to_string())
                .collect(),
        )
        .timeout(DOC_TIMEOUT),
    )
    .await
    .ok()?;
    if !doc.success {
        return None;
    }
    let end = doc.stdout.len().min(MAX_DOC_BYTES);
    let text = String::from_utf8_lossy(&doc.stdout[..end]);
    let discovered = parse_env_block(&text);
    if discovered.is_empty() {
        return None;
    }

    let version = probe_version(bin).await;
    let recommendations = build_recommendations();
    Some(merge_catalog(version, discovered, recommendations))
}

async fn probe_version(bin: &str) -> Option<String> {
    let output =
        exec(&ExecOptions::new(bin, vec!["--version".to_string()]).timeout(VERSION_TIMEOUT))
            .await
            .ok()?;
    if !output.success {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("agent-browser")?.trim();
        let version = rest.split_whitespace().next()?;
        version
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
            .then(|| version.to_string())
    })
}

/// CLI 文档里发现的一条变量（已过滤禁用项）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredEnv {
    pub key: String,
    /// 文档中的示例值（作为 hint 补充，不直接写进配置）
    pub example: String,
    pub desc: String,
}

/// 解析 `## Environment Variables` 下的 bash 代码块：`KEY="示例" # 说明`
pub(crate) fn parse_env_block(text: &str) -> Vec<DiscoveredEnv> {
    let mut lines = text.lines();
    if !lines.any(|line| line.trim() == "## Environment Variables") {
        return Vec::new();
    }
    let mut in_block = false;
    let mut result = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if !in_block {
            if trimmed.starts_with("```") {
                in_block = true;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            break;
        }
        if let Some(env) = parse_env_line(line) {
            result.push(env);
        }
    }
    result
}

/// 单行解析：只保留 `AGENT_BROWSER_` 前缀项。
///
/// 前缀是唯一硬约束（防注入系统环境变量劫持进程）；命中风险片段的变量照常进 DTO，
/// 由配置页附风险说明但仍可填写——CLI 本就支持这些能力，不做功能封禁。
pub(crate) fn parse_env_line(line: &str) -> Option<DiscoveredEnv> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (assignment, comment) = split_comment(line);
    let (key, raw_value) = assignment.split_once('=')?;
    let key = key.trim();
    if !key.starts_with("AGENT_BROWSER_") {
        return None;
    }
    Some(DiscoveredEnv {
        key: key.to_string(),
        example: unquote(raw_value.trim()),
        desc: comment.trim().to_string(),
    })
}

/// 按引号状态找行尾注释，避免误伤值里的 `#`
fn split_comment(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let mut quote = None;
    for (index, ch) in line.char_indices() {
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '#' && (index == 0 || bytes[index - 1].is_ascii_whitespace()) => {
                return (line[..index].trim_end(), &line[index + 1..]);
            }
            None => {}
        }
    }
    (line, "")
}

fn unquote(value: &str) -> String {
    if value.len() >= 2
        && let (Some(first), Some(last)) = (value.chars().next(), value.chars().last())
        && ((first == '"' && last == '"') || (first == '\'' && last == '\''))
    {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

/// 本机推荐值：浏览器可执行路径 + 服务进程已有的代理（带凭据的丢弃）
pub(crate) fn build_recommendations() -> HashMap<String, Vec<BrowserEnvOptionDTO>> {
    let mut result: HashMap<String, Vec<BrowserEnvOptionDTO>> = HashMap::new();

    let mut browsers = Vec::new();
    for (name, path) in browser_candidates() {
        if path.is_file() {
            browsers.push(BrowserEnvOptionDTO {
                value: path.to_string_lossy().into_owned(),
                label: name,
            });
        }
    }
    if !browsers.is_empty() {
        result.insert("AGENT_BROWSER_EXECUTABLE_PATH".to_string(), browsers);
    }

    let mut proxy = None;
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(raw) = std::env::var(key)
            && let Some(safe) = sanitize_proxy(&raw)
        {
            proxy = Some(BrowserEnvOptionDTO {
                value: safe,
                label: format!("服务进程 {key}"),
            });
            break;
        }
    }
    if let Some(proxy) = proxy {
        result.insert("AGENT_BROWSER_PROXY".to_string(), vec![proxy]);
    }
    result
}

/// 已知浏览器安装位置（只做 `is_file()` 判断，不 spawn、不读内容）
fn browser_candidates() -> Vec<(String, PathBuf)> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let mut candidates: Vec<(&str, PathBuf)> = Vec::new();

    if cfg!(target_os = "macos") {
        for (name, app) in [
            ("Microsoft Edge", "Microsoft Edge.app"),
            ("Google Chrome", "Google Chrome.app"),
            ("Chromium", "Chromium.app"),
            ("Brave Browser", "Brave Browser.app"),
        ] {
            let suffix = format!("Contents/MacOS/{}", name);
            candidates.push((name, PathBuf::from("/Applications").join(app).join(&suffix)));
            if !home.is_empty() {
                candidates.push((
                    name,
                    PathBuf::from(&home)
                        .join("Applications")
                        .join(app)
                        .join(&suffix),
                ));
            }
        }
    } else if cfg!(target_os = "linux") {
        for (name, path) in [
            ("Google Chrome", "/usr/bin/google-chrome"),
            ("Google Chrome stable", "/usr/bin/google-chrome-stable"),
            ("Chromium", "/usr/bin/chromium"),
            ("Chromium", "/usr/bin/chromium-browser"),
            ("Google Chrome", "/opt/google/chrome/chrome"),
            ("Microsoft Edge", "/usr/bin/microsoft-edge"),
            ("Brave Browser", "/usr/bin/brave-browser"),
        ] {
            candidates.push((name, PathBuf::from(path)));
        }
    } else if cfg!(target_os = "windows") {
        for (name, var, relative) in [
            (
                "Google Chrome",
                "ProgramFiles",
                "Google/Chrome/Application/chrome.exe",
            ),
            (
                "Microsoft Edge",
                "ProgramFiles",
                "Microsoft/Edge/Application/msedge.exe",
            ),
            (
                "Microsoft Edge",
                "ProgramFiles(x86)",
                "Microsoft/Edge/Application/msedge.exe",
            ),
        ] {
            if let Ok(root) = std::env::var(var) {
                candidates.push((name, PathBuf::from(root).join(relative.replace('/', "\\"))));
            }
        }
    }

    candidates
        .into_iter()
        .map(|(name, path)| (name.to_string(), path))
        .collect()
}

/// 代理推荐只回显无凭据的合法 URL；带账号密码的整条丢弃，绝不进前端
pub(crate) fn sanitize_proxy(raw: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    url.set_query(None);
    let value = url.to_string();
    Some(value.trim_end_matches('/').to_string())
}

/// 合并顺序：中文精选兜底项在前（保持原顺序），CLI 新发现项追加在后
pub(crate) fn merge_catalog(
    cli_version: Option<String>,
    discovered: Vec<DiscoveredEnv>,
    recommendations: HashMap<String, Vec<BrowserEnvOptionDTO>>,
) -> BrowserEnvProbeDTO {
    let mut entries = Vec::new();

    for spec in BROWSER_ENV_CATALOG {
        let dynamic = discovered.iter().find(|item| item.key == spec.key);
        entries.push(BrowserEnvEntryDTO {
            key: spec.key.to_string(),
            label: spec.label.to_string(),
            desc: spec.desc.to_string(),
            hint: dynamic
                .map(|item| merge_hint(spec.hint, &item.example))
                .unwrap_or_else(|| spec.hint.to_string()),
            choices: choices_to_dto(spec.kind),
            recommended: recommendations.get(spec.key).cloned().unwrap_or_default(),
            advanced: spec.advanced,
            from_cli: dynamic.is_some(),
            configurable: true,
        });
    }

    for item in discovered {
        if BROWSER_ENV_CATALOG.iter().any(|spec| spec.key == item.key) {
            continue;
        }
        // 风险项仍进目录且照常可填，只是附一条风险说明由管理员自行取舍
        let risk = browser_env_blocked_reason(&item.key);
        let desc = if let Some(reason) = risk {
            format!("⚠️ 风险提示：{reason}（CLI 原文：{}）", item.desc)
        } else if item.desc.is_empty() {
            item.key.clone()
        } else {
            item.desc.clone()
        };
        entries.push(BrowserEnvEntryDTO {
            key: item.key.clone(),
            label: dynamic_label(&item.key),
            desc,
            hint: if item.example.is_empty() {
                "—".to_string()
            } else {
                format!("示例：{}", item.example)
            },
            choices: Vec::new(),
            recommended: recommendations.get(&item.key).cloned().unwrap_or_default(),
            advanced: risk.is_some(),
            from_cli: true,
            configurable: true,
        });
    }

    BrowserEnvProbeDTO {
        cli_version,
        entries,
    }
}

fn merge_hint(static_hint: &str, example: &str) -> String {
    if example.is_empty() || static_hint.contains(example) {
        static_hint.to_string()
    } else {
        format!("{static_hint} · CLI 示例：{example}")
    }
}

fn choices_to_dto(kind: BrowserEnvKind) -> Vec<BrowserEnvOptionDTO> {
    match kind {
        BrowserEnvKind::Text => Vec::new(),
        BrowserEnvKind::Choice(options) => options
            .iter()
            .map(|(value, label)| BrowserEnvOptionDTO {
                value: (*value).to_string(),
                label: (*label).to_string(),
            })
            .collect(),
    }
}

fn dynamic_label(key: &str) -> String {
    key.trim_start_matches("AGENT_BROWSER_")
        .split('_')
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}
