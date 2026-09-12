//! 消息 @ 提及：前端渲染层
//!
//! 协议核心（解析 / 拼装 / 输入检测 / 提及提取 / 名字解析）已下沉到
//! `common::mention`，作为前后端共享的**单一事实源**——后端提取提及
//! （如 prompt 注入）与前端渲染走同一套解析，协议迭代不会产生差异。
//! 本模块只保留前端专属的部分：
//!
//! - pulldown-cmark 事件流拦截，把提及链接替换为 chip（[`transform_mentions`]）
//! - chip HTML 生成与 XSS 转义（[`render_mention_chip`]）
//! - 输入框光标读写（[`read_caret`] / [`restore_caret`]）——@ 查询的边界判定
//!   与插入位置都依赖真实光标，宿主 textarea 共用这两个 helper
//!
//! 下面的 `pub use` 把协议 API 原样转发，前端调用方（`mention_picker` /
//! `chat` / `markdown`）的 import 路径不受下沉影响。

pub use common::mention::{
    MentionKind, MentionQuery, MentionRef, apply_mention_pick, detect_mention_query,
    format_mention_ref, parse_mention_dest, remove_mention_token, resolve_display_name,
};

use pulldown_cmark::{Event, Tag, TagEnd};
use wasm_bindgen::JsCast;

use common::api::MessageListItem;

use crate::utils::message::{NameMap, resolve_receiver_name};

/// 按 DOM id 取输入框元素
fn caret_element(id: &str) -> Option<web_sys::HtmlTextAreaElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(id)?
        .dyn_into::<web_sys::HtmlTextAreaElement>()
        .ok()
}

/// UTF-16 code unit 下标 → UTF-8 字节下标（用于把 DOM 光标换算成 Rust 字符串下标）
///
/// DOM 的 `selectionStart` 按 **UTF-16 code unit** 计数（一个汉字 = 1），
/// 而 Rust 字符串下标是 **字节**（一个汉字 = 3）。直接把 `selectionStart`
/// 当字节用，中文场景下光标会系统性「落后」：`我们@` 的 selectionStart 是 3，
/// 取 `&text[..3]` 得到的是 `我` —— 里面根本没有 `@`，于是菜单永远不弹。
/// 这正是「@ 打在开头能用、前面有中文就唤不出」的根因。
///
/// 落在代理对中间时回退到该字符起始字节（半个字符无法表达，取靠左的合法边界）。
fn utf16_to_byte(s: &str, units: usize) -> usize {
    let mut acc = 0usize;
    for (byte_idx, c) in s.char_indices() {
        if acc >= units {
            return byte_idx;
        }
        acc += c.len_utf16();
    }
    s.len()
}

/// UTF-8 字节下标 → UTF-16 code unit 下标（[`utf16_to_byte`] 的逆运算）
///
/// [`restore_caret`] 拿到的是 Rust 侧算出的字节偏移，写回 DOM 前必须换算回去，
/// 否则插入提及后光标会停在错误位置（中文越多偏得越离谱）。
fn byte_to_utf16(s: &str, byte: usize) -> usize {
    let mut acc = 0usize;
    for (byte_idx, c) in s.char_indices() {
        if byte_idx >= byte {
            return acc;
        }
        acc += c.len_utf16();
    }
    acc
}

/// 读取输入框光标位置（**字节下标**，可直接用于 Rust 字符串切片）
///
/// 内部把 DOM 的 UTF-16 下标经 [`utf16_to_byte`] 换算，调用方无需自行处理编码。
/// 读不到（元素不存在 / 浏览器拒绝）返回 `None`，调用方应回退到文本末尾——
/// 表现为「不弹菜单」，而不是错位插入。
pub fn read_caret(id: &str) -> Option<usize> {
    let el = caret_element(id)?;
    let units = el.selection_start().ok().flatten()? as usize;
    Some(utf16_to_byte(&el.value(), units))
}

/// 把光标放回输入框指定位置（并重新聚焦）
///
/// `pos` 是 Rust 侧的**字节下标**，内部换算成 DOM 的 UTF-16 下标后再写回。
///
/// 受控 textarea 的值由框架在事件结束后统一写回，因此用 0ms 定时器把恢复动作
/// 推到 DOM 更新之后，否则会被随后写入的 value 冲掉。
pub fn restore_caret(id: &'static str, pos: usize) {
    gloo_timers::callback::Timeout::new(0, move || {
        if let Some(el) = caret_element(id) {
            let units = byte_to_utf16(&el.value(), pos) as u32;
            let _ = el.set_selection_range(units, units);
            let _ = el.focus();
        }
    })
    .forget();
}

/// HTML 转义（chip 的展示名与 id 拼进 HTML 前必过）
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// 渲染提及 chip 的 HTML（供 `Event::InlineHtml` 注入）
///
/// 带 `data-mention-kind` / `data-mention-id`，供样式与后续点击跳转挂钩。
/// 跨组织联邦提及（`agent:<id>@<org_id>`）叠加 `mention-federated` 修饰类
/// （虚线边框）并携带 `data-mention-org`。
pub fn render_mention_chip(m: &MentionRef, display_name: &str) -> String {
    let kind = m.kind.as_str();
    let cls = m.kind.chip_class();
    let id = escape_html(&m.id);
    let name = escape_html(display_name);
    match &m.org {
        Some(org) => {
            let org = escape_html(org);
            format!(
                r#"<span class="mention-chip {cls} mention-federated" data-mention-kind="{kind}" data-mention-id="{id}" data-mention-org="{org}" title="{kind}: {id}@{org}">@{name}</span>"#
            )
        }
        None => format!(
            r#"<span class="mention-chip {cls}" data-mention-kind="{kind}" data-mention-id="{id}" title="{kind}: {id}">@{name}</span>"#
        ),
    }
}

/// 消息接收方的「提及 chip」HTML（气泡头部拼出「这条消息发给谁」）
///
/// 群聊（项目会话）里消息不止你 ↔ 一个 Agent 两条线，Agent 之间也会互相说话。
/// 在气泡头部用**与正文 @ 提及完全一致**的写法标出接收方（同样的 `.mention-chip`
/// 样式、同样的 `@名` 形态），一眼就能看出「谁在跟谁聊」，再配合旁观消息的
/// 降透明度（`involves_user`），就不会觉得每条都冲着自己来。
///
/// 接收方是 Agent 时直接复用 [`render_mention_chip`]（info 配色 + `data-mention-*`，
/// 与正文提及视觉同源）；是用户 / 系统时降级为基础 chip ——
/// `MentionKind` 只有 Agent / Task / Project 三种，用户不在提及协议里。
///
/// `to_id` 为空（如未锚定收件人的默认对话）时返回 `None`，不渲染空 chip。
pub fn receiver_mention_html(
    msg: &MessageListItem,
    agents: &NameMap,
    users: &NameMap,
) -> Option<String> {
    if msg.to_id.trim().is_empty() {
        return None;
    }
    let name = resolve_receiver_name(msg, agents, users);
    match msg.to_role {
        1 => Some(render_mention_chip(
            &MentionRef {
                kind: MentionKind::Agent,
                id: msg.to_id.clone(),
                org: None,
            },
            &name,
        )),
        _ => Some(format!(
            r#"<span class="mention-chip" title="{}">@{}</span>"#,
            escape_html(&msg.to_id),
            escape_html(&name)
        )),
    }
}

/// 在 pulldown-cmark 事件流里把提及链接替换为 chip
///
/// 顺带承担源文 HTML 的转义（`Html` / `InlineHtml` → `Text`），
/// 因此调用方不需要再单独做一层转义映射。
///
/// 提及链接内部的事件会被吞掉并收集为展示名快照 —— Markdown 不允许链接嵌套链接，
/// 所以用单层状态机即可，无需维护深度栈。
pub fn transform_mentions<'a, I>(events: I, agents: Option<&NameMap>) -> Vec<Event<'a>>
where
    I: Iterator<Item = Event<'a>>,
{
    let mut out: Vec<Event<'a>> = Vec::new();
    // 非空表示当前处于提及链接内部
    let mut pending: Option<MentionRef> = None;
    let mut name_buf = String::new();

    for event in events {
        if pending.is_some() {
            let is_end = matches!(event, Event::End(TagEnd::Link));
            match &event {
                Event::Text(t) | Event::Code(t) => name_buf.push_str(t),
                Event::SoftBreak | Event::HardBreak => name_buf.push(' '),
                _ => {}
            }
            if is_end {
                let m = pending.take().unwrap_or(MentionRef {
                    kind: MentionKind::Agent,
                    id: String::new(),
                    org: None,
                });
                let snapshot = normalize_snapshot(&name_buf, &m);
                let name = resolve_display_name(&m, &snapshot, agents);
                out.push(Event::InlineHtml(render_mention_chip(&m, &name).into()));
            }
            continue;
        }

        match event {
            // 源文原始 HTML 降级为纯文本（push_html 会自动转义），保证注入安全
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Event::Text(raw)),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => match parse_mention_dest(&dest_url) {
                Some(m) => {
                    pending = Some(m);
                    name_buf.clear();
                }
                None => out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                })),
            },
            other => out.push(other),
        }
    }

    // 异常兜底：链接未闭合时（理论上不会发生）至少把已收集的内容吐出来
    if let Some(m) = pending.take() {
        let snapshot = normalize_snapshot(&name_buf, &m);
        let name = resolve_display_name(&m, &snapshot, agents);
        out.push(Event::InlineHtml(render_mention_chip(&m, &name).into()));
    }

    out
}

/// 规整快照名：去掉首尾空白与多余的 `@` 前缀
///
/// 链接文本为空（用户手打了 `[](agent:agt_7f3)`）时回退到 id，
/// 保证 chip 至少有个可读内容。
fn normalize_snapshot(raw: &str, m: &MentionRef) -> String {
    let trimmed = raw.trim().trim_start_matches('@').trim();
    if trimmed.is_empty() {
        m.id.clone()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端到端：Markdown 源文 → HTML，提及链接变成 chip
    fn render(md: &str, agents: Option<&NameMap>) -> String {
        let mut options = pulldown_cmark::Options::empty();
        options.insert(pulldown_cmark::Options::ENABLE_TABLES);
        let parser = pulldown_cmark::Parser::new_ext(md, options);
        let events = transform_mentions(parser, agents);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, events.into_iter());
        html
    }

    /// 光标单位换算：DOM 的 UTF-16 下标 → Rust 字节下标
    ///
    /// 回归守卫：曾把 `selectionStart` 直接当字节用，导致中文前缀后打 `@`
    /// 一律唤不出菜单（`@` 被截在切片之外）。
    #[test]
    fn caret_utf16_to_byte_conversion() {
        let s = "我们@";
        // DOM 光标停在 `@` 之后 = 3 个 code unit，字节下标是 7
        assert_eq!(utf16_to_byte(s, 3), 7);
        let q = detect_mention_query(s, utf16_to_byte(s, 3)).expect("中文前缀后打 @ 应触发");
        assert_eq!(q.query, "");
        // 不换算就会漏掉：拿 3 当字节只截出「我」，里面根本没有 @
        assert!(detect_mention_query(s, 3).is_none());
    }

    #[test]
    fn caret_byte_to_utf16_roundtrip() {
        let s = "看下@张伟 进度";
        for (byte_idx, _) in s.char_indices() {
            assert_eq!(utf16_to_byte(s, byte_to_utf16(s, byte_idx)), byte_idx);
        }
        assert_eq!(byte_to_utf16("我们@abc", 7), 3);
    }

    #[test]
    fn render_mention_chip_escapes_html() {
        let m = MentionRef {
            kind: MentionKind::Agent,
            id: "agt_1".to_string(),
            org: None,
        };
        let html = render_mention_chip(&m, "<img src=x onerror=alert(1)>");
        assert!(html.contains("&lt;img"));
        assert!(!html.contains("<img"));
        assert!(html.contains("data-mention-id=\"agt_1\""));
        assert!(html.contains("mention-agent"));
    }

    #[test]
    fn transform_renders_mention_as_chip() {
        let html = render("[@张伟](agent:agt_7f3) 看下进度", None);
        assert!(html.contains("mention-chip"));
        assert!(html.contains("mention-agent"));
        assert!(html.contains("data-mention-id=\"agt_7f3\""));
        // 原链接语法不出现在产物里
        assert!(!html.contains("agent:agt_7f3\""));
        assert!(html.contains("看下进度"));
    }

    #[test]
    fn transform_supports_multiple_task_mentions() {
        // @任务可多选：两个任务提及应各自渲染成 chip
        let html = render("[@A](task:t1) 和 [@B](task:t2) 都阻塞了", None);
        assert_eq!(html.matches("mention-task").count(), 2);
        assert!(html.contains("data-mention-id=\"t1\""));
        assert!(html.contains("data-mention-id=\"t2\""));
    }

    #[test]
    fn transform_leaves_normal_links_untouched() {
        let html = render("[文档](https://example.com) 参考", None);
        assert!(html.contains("<a href=\"https://example.com\">"));
        assert!(!html.contains("mention-chip"));
    }

    #[test]
    fn transform_still_escapes_raw_html() {
        // 源文 HTML 仍要被转义（不能因为加了提及链路就放开 XSS）
        let html = render("<script>alert(1)</script>", None);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn transform_uses_directory_name_when_available() {
        let mut agents = NameMap::new();
        agents.insert("agt_7f3".to_string(), "李雷".to_string());
        let html = render("[@张伟](agent:agt_7f3) 你好", Some(&agents));
        // 目录里有实时名时，覆盖文本里的快照名
        assert!(html.contains("@李雷"));
        assert!(!html.contains("@张伟"));
    }

    #[test]
    fn reexports_match_common_protocol() {
        // 前端转发与 common 协议保持同源：拼装出来的语法能被解析还原
        // （format_mention 仅有本测试使用，不进 pub use 转发面，避免 bin 构建未用导入）
        let token = common::mention::format_mention(MentionKind::Project, "prj_1", "平台");
        let parsed = token
            .split_once("](")
            .and_then(|(_, dest)| parse_mention_dest(dest.trim_end_matches(')')));
        assert_eq!(
            parsed,
            Some(MentionRef {
                kind: MentionKind::Project,
                id: "prj_1".to_string(),
                org: None
            })
        );
    }

    /// 气泡头部的接收方 chip：Agent 走复用 path（info 配色），用户降级为基础 chip，
    /// 无收件人时不渲染（避免出现空的 `@`）
    #[test]
    fn receiver_chip_renders_agent_and_user() {
        let mut agents = NameMap::new();
        agents.insert("agt_9".to_string(), "李雷".to_string());
        let users = NameMap::new();

        let base = MessageListItem {
            message_id: "m1".to_string(),
            project_id: Some("prj_1".to_string()),
            task_id: None,
            from_id: "user".to_string(),
            from_role: 0,
            to_id: "agt_9".to_string(),
            to_role: 1,
            message_type: 0,
            status: 3,
            content: "进度如何".to_string(),
            reply_to_id: None,
            root_id: None,
            created_at: 0,
            file_type: None,
            file_meta: None,
        };

        let html = receiver_mention_html(&base, &agents, &users).unwrap();
        assert!(html.contains("@李雷"));
        assert!(html.contains("mention-agent"));
        assert!(html.contains("data-mention-id=\"agt_9\""));

        // 接收方是用户 → 基础 chip，不套 Agent 配色
        let mut to_user = base.clone();
        to_user.from_id = "agt_9".to_string();
        to_user.from_role = 1;
        to_user.to_id = "u_1".to_string();
        to_user.to_role = 0;
        let html = receiver_mention_html(&to_user, &agents, &users).unwrap();
        assert!(html.contains("@我"));
        assert!(!html.contains("mention-agent"));

        // 无收件人（默认对话未锚定）→ 不渲染
        let mut no_to = base;
        no_to.to_id = String::new();
        assert!(receiver_mention_html(&no_to, &agents, &users).is_none());
    }
}
