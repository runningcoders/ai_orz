//! 消息 @ 提及：前端渲染层
//!
//! 协议核心（解析 / 拼装 / 输入检测 / 提及提取 / 名字解析）已下沉到
//! `common::mention`，作为前后端共享的**单一事实源**——后端提取提及
//! （如 prompt 注入）与前端渲染走同一套解析，协议迭代不会产生差异。
//! 本模块只保留前端专属的渲染逻辑：
//!
//! - pulldown-cmark 事件流拦截，把提及链接替换为 chip（[`transform_mentions`]）
//! - chip HTML 生成与 XSS 转义（[`render_mention_chip`]）
//!
//! 下面的 `pub use` 把协议 API 原样转发，前端调用方（`mention_picker` /
//! `chat` / `markdown`）的 import 路径不受下沉影响。

pub use common::mention::{
    MentionKind, MentionQuery, MentionRef, apply_mention_pick, detect_mention_query,
    format_mention, parse_mention_dest, remove_mention_token, resolve_display_name,
};

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::utils::message::NameMap;

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
pub fn render_mention_chip(m: &MentionRef, display_name: &str) -> String {
    let kind = m.kind.as_str();
    let cls = m.kind.chip_class();
    let id = escape_html(&m.id);
    let name = escape_html(display_name);
    format!(
        r#"<span class="mention-chip {cls}" data-mention-kind="{kind}" data-mention-id="{id}" title="{kind}: {id}">@{name}</span>"#
    )
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

    #[test]
    fn render_mention_chip_escapes_html() {
        let m = MentionRef {
            kind: MentionKind::Agent,
            id: "agt_1".to_string(),
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
        let token = format_mention(MentionKind::Project, "prj_1", "平台");
        let parsed = token
            .split_once("](")
            .and_then(|(_, dest)| parse_mention_dest(dest.trim_end_matches(')')));
        assert_eq!(
            parsed,
            Some(MentionRef {
                kind: MentionKind::Project,
                id: "prj_1".to_string()
            })
        );
    }
}
