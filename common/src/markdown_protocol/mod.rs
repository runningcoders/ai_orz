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
//! ## 解析策略（决策点③，批1 仅预留；Mixed 写入路径已接通）
//!
//! [`ParsePolicy::Lenient`] 宽容解析：全 Text 类统一启用解析、未识别语法自然
//! 降级。`MessageType::Mixed`（=13）批1 只是枚举预留，**现在已有生产写入路径**：
//! 站内前端与渠道入站用 `append_refs` 把 `[文件名](attachment:id)` 写进请求
//! 正文，投递域按正文含资源引用归一升级为 Mixed；展示与唤醒都从正文解析引用。
//! （`ParsePolicy` / `ProtocolEvent::Image` 本身仍未接线，属独立预留。）

pub mod component;
pub mod transform;

pub use component::{
    ArtifactComponent, AttachmentComponent, MentionComponent, ProtocolComponent, ProtocolRef,
    ProtocolRegistry, ProtocolRegistryBuilder, ResolvedPayload,
};
pub use transform::{TransformOptions, transform};

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

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
pub fn normalize_snapshot(raw: &str, fallback_id: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('@').trim();
    if trimmed.is_empty() {
        fallback_id.to_string()
    } else {
        trimmed.to_string()
    }
}

/// 链接文本 → 纯文本（复现 [`transform`] 对链接内 Text/Code 事件的收集口径）
///
/// `transform` 在拦截协议引用时，链接文本来自 pulldown-cmark 的语义事件流，
/// 结构化标记（`**加粗**` / `*斜体*` / `` `代码` ``）会被解析掉，只收集 Text / Code
/// 内容，软/硬换行折成空格。`extract_refs_with_spans` 走纯字节扫描拿到的是**原始**
/// 链接文本，若要让占位文本与 transform 路径逐字一致，需对这一小段做同口径消解。
///
/// ⚠️ 只作用于链接文本片段（通常几个字），**不是**整段正文——不对正文做 parse/reserialize，
/// 因此不会引入有序列表重编号 / 转义反转 / 围栏破损等 round-trip 有损问题。
fn link_text_plain(raw: &str) -> String {
    let mut out = String::new();
    for event in Parser::new(raw) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
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

/// 拼装口：引用 + 展示名 → 协议文本（批1 提供能力，现为发送链唯一拼装口）
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

/// 把一组已拼装好的引用并入正文（空正文只留引用）
///
/// 发送链唯一拼装入**口**：站内前端（`MessageChat`）与渠道入站
/// （`producer::message_channel::compose_inbound_content`）都在**入库前**调用它
/// 把附件引用写进请求正文，投递域只对漏写声明的 id 做兜底补拼。两处共用同一
/// 拼装口，正文形态才不会分叉。正文与引用之间留一个空行（Markdown 段落边界），
/// 多个引用之间用空格分隔，保证 `extract_refs` 与前端渲染都能稳定识别。
pub fn append_refs(base: &str, refs: &[String]) -> String {
    if refs.is_empty() {
        return base.to_string();
    }
    let block = refs.join(" ");
    let base = base.trim();
    if base.is_empty() {
        block
    } else {
        format!("{base}\n\n{block}")
    }
}

/// 从文本提取全部协议引用及链接文本快照（extract 能力，零解析器开销）
///
/// 轻量扫描 `[...](scheme:id[@org])` 链接语法，扫描规则与
/// [`crate::mention::extract_mentions_with_text`] 同源（`](` 锚点回溯 `[`、
/// dest 含空白提前放弃、已转义 `]` 不闭合）；本函数不触碰 mention 热路径。
///
/// 返回 `(ProtocolRef, 链接文本)`：链接文本剥掉前导 `@` 后作为展示名快照回退值。
pub fn extract_refs(text: &str) -> Vec<(ProtocolRef, String)> {
    // 原口径：链接文本仅剥前导 @（与 extract_mentions_with_text 对拍），保持零行为变化
    scan_refs(text)
        .into_iter()
        .map(|(r, raw, _)| (r, raw.trim_start_matches('@').to_string()))
        .collect()
}

/// 扫描骨架：产出协议引用、原始链接文本、以及整个链接语法的字节 span。
///
/// 扫描规则：`](` 锚点回溯 `[`、`dest` 含空白提前放弃、已转义 `]` 不闭合——与
/// [`crate::mention::extract_mentions_with_text`] 同源。`snapshot` 加工交给两个
/// 公开函数各自决定（`extract_refs` 保持旧口径，本函数的 span 变体用 normalize
/// 口径），避免口径漂移。
fn scan_refs(text: &str) -> Vec<(ProtocolRef, &str, std::ops::Range<usize>)> {
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
            // span 覆盖整个 `[名](scheme:id)`，即 open..=rel；raw 为链接文本
            out.push((r, &text[open + 1..bracket], open..rel + 1));
        }
        i = rel + 1;
    }
    out
}

/// 带字节 span 的引用提取（出站降级等"只替换引用、原文逐字不动"场景）
///
/// 扫描规则与 [`extract_refs`] 完全同源（共享 [`scan_refs`]），额外返回每个协议
/// 引用 `[名](scheme:id[@org])` 在原文中的字节范围 `start..end`（左闭右开，覆盖
/// 整个链接语法）。调用方可据此只替换引用 span，避免整段 Markdown 经解析器
/// rewrite 造成有序列表重编号 / 转义反转 / 代码块围栏破损等有损改写。
///
/// snapshot 口径与 [`transform`] 一致：先对链接文本做 [`link_text_plain`] 消解
/// （剥掉 `**加粗**` 等结构化标记），再经 [`normalize_snapshot`]（去首尾空白与 `@`、
/// 空则回退 id），保证替换后的占位文本与事件流降级路径逐字相同。
///
/// ⚠️ 仅收录语法合法的协议引用（与 `extract_refs` 一致，未注册 scheme 也会收录，
/// 由调用方按 `ProtocolRegistry::contains` 过滤）。span 为 UTF-8 字节偏移，可
/// 直接用于 `String::replace_range`。
pub fn extract_refs_with_spans(text: &str) -> Vec<(ProtocolRef, String, std::ops::Range<usize>)> {
    scan_refs(text)
        .into_iter()
        .map(|(r, raw, span)| {
            let snapshot = normalize_snapshot(&link_text_plain(raw), &r.id);
            (r, snapshot, span)
        })
        .collect()
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
