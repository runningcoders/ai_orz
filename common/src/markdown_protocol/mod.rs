//! Markdown 解析协议（批1 协议基座）：parse / transform / extract 三能力
//!
//! 协议形态：标准 CommonMark 链接语法承载统一资源引用，dest 为
//! `scheme:id[@org]`——mention 族（`agent:` / `task:` / `project:`，单一事实源
//! 在 [`crate::mention`]，本模块零复制）与资源引用（`attachment:` / `artifact:`，
//! 批2 起落地）同构扩展；未注册 / 未识别形态一律降级普通链接（降级安全哲学）。
//!
//! ## 三能力
//!
//! - **parse**：[`parse_protocol`] → [`ProtocolEvent`] 语义事件流（后端 / 非
//!   渲染消费者，批3 混排主线、批4 vision 的入口）；
//! - **transform**：[`transform`] 事件流改写（唯一拦截核，前端渲染薄封装与
//!   后端降级共用，见 [`transform`] 模块）；
//! - **extract**：[`extract_refs`] 轻量扫描（零解析器开销，consumer 热路径
//!   适配候选；批1 不切后端热路径，[`crate::mention::extract_mentions`] 不动）。
//!
//! ## 注册制（决策点①）
//!
//! [`ProtocolRegistry`] 是拦截资格的唯一事实源：**扩展 = 一组件 + 一行注册**
//! （见 [`component::ProtocolComponent`]），解析器本体零改动。
//!
//! ## 解析策略（决策点③，批1 仅预留）
//!
//! [`ParsePolicy::Lenient`] 宽容解析：全 Text 类统一启用解析、未识别语法自然
//! 降级；`MessageType::Mixed`（=13）仅作语义标记，批1 无生产写入路径、无任何
//! 消费点按 Mixed 分流（发送链改造归批3）。

pub mod component;
pub mod transform;

pub use component::{
    ArtifactComponent, AttachmentComponent, MentionComponent, ProtocolComponent, ProtocolRef,
    ProtocolRegistry, ProtocolRegistryBuilder, ResolvedPayload,
};
pub use transform::{TransformOptions, transform};

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::mention::{format_mention_ref, parse_mention_dest};

/// 语义事件流（parse 能力的产出）
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolEvent<'a> {
    /// 非协议内容原样透传（普通文本 / 结构事件 / 未注册 scheme 的普通链接，
    /// 含 Html 语义事件——语义提取不吞 HTML，防 XSS 由渲染层负责）
    Plain(Event<'a>),
    /// 已注册 scheme 的链接引用 + 链接文本快照
    Ref(ProtocolRef, String),
    /// 已注册 scheme 的图像内联（`![快照](scheme:id)`，批3 图文混排入口）
    Image(ProtocolRef, String),
}

/// 解析策略（决策点③落点：宽容解析；批1 仅常量预留，不接线不启用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsePolicy {
    /// 宽容解析：全 Text 类统一启用解析，未识别语法自然降级为普通链接 / 纯文本
    Lenient,
}

/// 默认解析策略（宽容解析常量；批1 无消费点，批3 主线接线）
pub const DEFAULT_PARSE_POLICY: ParsePolicy = ParsePolicy::Lenient;

/// 规整快照名：去首尾空白与 `@` 前缀；空文本回退到 id
///
/// 与前端 `normalize_snapshot` 同口径（对拍测试守护），保证迁移零行为变化。
pub(crate) fn normalize_snapshot(raw: &str, fallback_id: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('@').trim();
    if trimmed.is_empty() {
        fallback_id.to_string()
    } else {
        trimmed.to_string()
    }
}

/// dest → [`ProtocolRef`]（registry 感知口径，transform / parse 用）
///
/// mention 族以 [`parse_mention_dest`] 严格规则为准（org 仅 agent）；其余 scheme
/// 仅做语法校验 + 注册表查表，带 `@` / 空白 id / 未注册一律 `None`（降级普通链接）。
pub(crate) fn parse_ref_dest(dest: &str, registry: &ProtocolRegistry) -> Option<ProtocolRef> {
    if let Some(m) = parse_mention_dest(dest) {
        let r = ProtocolRef::from(&m);
        if registry.contains(&r.scheme) {
            return Some(r);
        }
        return None;
    }
    let (scheme, id) = dest.split_once(':')?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    if !registry.contains(scheme) {
        return None;
    }
    // org 后缀仅 agent 提及语义（上一分支已覆盖）；其余 scheme 带 @ 一律降级
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains('@') {
        return None;
    }
    Some(ProtocolRef {
        scheme: scheme.to_string(),
        id: id.to_string(),
        org: None,
    })
}

/// dest → [`ProtocolRef`]（extract 泛化口径：scheme 语法合法性校验，无 registry 依赖）
///
/// 扫描器不持有 registry，因此对**语法合法**的 `scheme:id` 一律收录（含未注册
/// scheme），消费方按需经 [`ProtocolRegistry::contains`] 过滤；mention 族仍以
/// [`parse_mention_dest`] 严格规则为准，保证与 [`crate::mention::extract_mentions`]
/// 在 mention 形态上同输出（对拍测试守护）。
fn classify_ref_dest(dest: &str) -> Option<ProtocolRef> {
    if let Some(m) = parse_mention_dest(dest) {
        return Some(ProtocolRef::from(&m));
    }
    let (scheme, id) = dest.split_once(':')?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains('@') {
        return None;
    }
    Some(ProtocolRef {
        scheme: scheme.to_string(),
        id: id.to_string(),
        org: None,
    })
}

/// 拼装口：引用 + 展示名 → 协议文本（批3 发送链消费，批1 仅提供能力）
///
/// mention 形态复用 [`format_mention_ref`]（含 `@` 触发符与 `\` `[` `]` 转义规则，
/// 单一事实源零复制）；其余形态同规则转义、无 `@` 前缀。非提及形态的 `org` 段
/// 当前无解析语义（预留），按原样输出。
pub fn format_ref(r: &ProtocolRef, name: &str) -> String {
    if let Some(m) = r.to_mention() {
        return format_mention_ref(&m, name);
    }
    let safe_name = name
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let dest = match &r.org {
        Some(org) => format!("{}:{}@{}", r.scheme, r.id, org),
        None => format!("{}:{}", r.scheme, r.id),
    };
    format!("[{safe_name}]({dest})")
}

/// 从文本提取全部协议引用及链接文本快照（extract 能力，零解析器开销）
///
/// 轻量扫描 `[...](scheme:id[@org])` 链接语法，扫描规则与
/// [`crate::mention::extract_mentions_with_text`] 同源（`](` 锚点回溯 `[`、
/// dest 含空白提前放弃、已转义 `]` 不闭合）；本函数不触碰 mention 热路径。
///
/// 返回 `(ProtocolRef, 链接文本)`：链接文本剥掉前导 `@` 后作为展示名快照回退值。
pub fn extract_refs(text: &str) -> Vec<(ProtocolRef, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("](") {
        let bracket = i + off;
        // 回溯找链接文本起点 `[`（取 `](` 之前最近的那个，排除孤立的 "](...)"）
        let Some(open) = text[..bracket].rfind('[') else {
            i = bracket + 2;
            continue;
        };
        // 链接文本（展示快照）：剥掉前导 @（它是提及触发符，不是实体名本身）
        let snapshot = text[open + 1..bracket].trim_start_matches('@').to_string();
        let dest_start = bracket + 2;
        let Some(rel) = text[dest_start..]
            .find(|c: char| c == ')' || c.is_whitespace())
            .map(|p| dest_start + p)
        else {
            i = dest_start;
            continue;
        };
        if bytes[rel] != b')' {
            // dest 含空白：不是合法链接，跳过
            i = rel + 1;
            continue;
        }
        if let Some(r) = classify_ref_dest(&text[dest_start..rel]) {
            out.push((r, snapshot));
        }
        i = rel + 1;
    }
    out
}

/// Markdown 源文 → 语义事件流（parse 能力）
///
/// 基于 pulldown-cmark 默认选项解析后按注册表分流：已注册 scheme 的链接 / 图像
/// 收敛为 [`ProtocolEvent::Ref`] / [`ProtocolEvent::Image`]（文本快照口径与
/// [`transform`] 一致），其余原样透传为 [`ProtocolEvent::Plain`]。
pub fn parse_protocol<'a>(md: &'a str, registry: &ProtocolRegistry) -> Vec<ProtocolEvent<'a>> {
    let parser = pulldown_cmark::Parser::new(md);
    let mut out: Vec<ProtocolEvent<'a>> = Vec::new();
    // (引用, 是否图像, 文本缓冲)
    let mut pending: Option<(ProtocolRef, bool, String)> = None;

    for event in parser {
        if pending.is_some() {
            let mut is_end = false;
            if let Some(p) = pending.as_mut() {
                match event {
                    Event::End(TagEnd::Link) if !p.1 => is_end = true,
                    Event::End(TagEnd::Image) if p.1 => is_end = true,
                    Event::Text(t) | Event::Code(t) => p.2.push_str(&t),
                    Event::SoftBreak | Event::HardBreak => p.2.push(' '),
                    _ => {}
                }
            }
            if is_end {
                let (r, is_image, buf) = pending.take().expect("pending 已判非空");
                let snapshot = normalize_snapshot(&buf, &r.id);
                out.push(if is_image {
                    ProtocolEvent::Image(r, snapshot)
                } else {
                    ProtocolEvent::Ref(r, snapshot)
                });
            }
            continue;
        }
        match event {
            Event::Start(tag @ (Tag::Link { .. } | Tag::Image { .. })) => {
                let dest = match &tag {
                    Tag::Link { dest_url, .. } => dest_url.to_string(),
                    Tag::Image { dest_url, .. } => dest_url.to_string(),
                    _ => unreachable!("上一模式已限定 Link | Image"),
                };
                if let Some(r) = parse_ref_dest(&dest, registry) {
                    pending = Some((r, matches!(tag, Tag::Image { .. }), String::new()));
                } else {
                    out.push(ProtocolEvent::Plain(Event::Start(tag)));
                }
            }
            other => out.push(ProtocolEvent::Plain(other)),
        }
    }

    // 异常兜底（parser 语义上不会产生未闭合链接，防御性保留）
    if let Some((r, is_image, buf)) = pending.take() {
        let snapshot = normalize_snapshot(&buf, &r.id);
        out.push(if is_image {
            ProtocolEvent::Image(r, snapshot)
        } else {
            ProtocolEvent::Ref(r, snapshot)
        });
    }
    out
}
#[cfg(test)]
#[path = "markdown_protocol_tests.rs"]
mod tests;
