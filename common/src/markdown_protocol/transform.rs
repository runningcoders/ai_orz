//! 事件流改写器：transform 能力（泛化自前端 transform_mentions）
//!
//! [`transform`] 是 Markdown 事件流的**唯一拦截核**：`Start(Tag::Link | Tag::Image)`
//! 且 dest 的 scheme 已注册 → 进入 pending 收集文本 → 匹配 `End` → 调用方 render
//! 闭包产出事件；调用方返回 `None`（或 dest 未注册）→ 按普通链接 / 图像原样透传；
//! 链接未闭合时兜底吐已收集内容（与前端现行为一致）。
//!
//! ## 设计边界
//!
//! - Markdown 链接不允许嵌套链接，单层状态机即可，无需深度栈；
//! - 本函数只做**协议识别与拦截**，不产生 HTML——chip HTML 等渲染细节归调用方
//!   （前端 render_mention_chip / 后端 render_text），保持分层；
//! - [`TransformOptions::demote_raw_html`]：前端 `true`（延续既有防 XSS 行为，
//!   `Html`/`InlineHtml` 降级为 `Text`），后端 `false` 原样保留。

use pulldown_cmark::{Event, Tag, TagEnd};

use super::component::{ProtocolRef, ProtocolRegistry};
use super::{normalize_snapshot, parse_ref_dest};

/// 改写选项（批1 仅一个开关，批2/3 按需扩展）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransformOptions {
    /// `Html` / `InlineHtml` 降级为 `Text`（前端 = true 防 XSS；后端 = false 原样保留）
    pub demote_raw_html: bool,
}

/// 在事件流里拦截已注册 scheme 的链接 / 图像，交由 render 闭包产出事件
///
/// - `events`：上游 pulldown-cmark 事件流；
/// - `registry`：拦截资格事实源（scheme 未注册 = 降级普通链接）；
/// - `opts`：改写选项（原始 HTML 降级开关）；
/// - `render`：命中时的产出闭包，入参 `(引用, 文本快照, 是否图像)`，返回
///   `Some(事件)` 收编、返回 `None` 时按原始链接形态透传（快照文本替代内层事件）。
///
/// 命中拦截后，链接 / 图像内部事件被吞并收集为文本快照（Text/Code/软硬换行参与
/// 拼接，与前端现行为一致）；图像分支为批3 `![预览](attachment:id)` 图像内联预留，
/// 零新语法。
pub fn transform<'a, I, F>(
    events: I,
    registry: &ProtocolRegistry,
    opts: &TransformOptions,
    mut render: F,
) -> Vec<Event<'a>>
where
    I: Iterator<Item = Event<'a>>,
    F: FnMut(&ProtocolRef, &str, bool) -> Option<Event<'a>>,
{
    let mut out: Vec<Event<'a>> = Vec::new();
    // 非空 = 当前处于已注册协议的链接 / 图像内部：(引用, 是否图像, 原始 Start Tag（兜底重建用）, 文本缓冲)
    let mut pending: Option<(ProtocolRef, bool, Tag<'a>, String)> = None;

    for event in events {
        if pending.is_some() {
            let mut is_end = false;
            {
                let p = pending.as_mut().expect("pending 已判非空");
                match event {
                    Event::End(TagEnd::Link) if !p.1 => is_end = true,
                    Event::End(TagEnd::Image) if p.1 => is_end = true,
                    Event::Text(t) | Event::Code(t) => p.3.push_str(&t),
                    Event::SoftBreak | Event::HardBreak => p.3.push(' '),
                    // 其余事件（含嵌套图像的结构事件）吞掉，仅其 Text 参与快照
                    _ => {}
                }
            }
            if is_end {
                let (r, is_image, start_tag, name_buf) = pending.take().expect("pending 已判非空");
                let snapshot = normalize_snapshot(&name_buf, &r.id);
                match render(&r, &snapshot, is_image) {
                    Some(ev) => out.push(ev),
                    None => {
                        // 调用方放弃拦截：按原始链接形态吐回（Start 原样 + 快照文本 + End）
                        out.push(Event::Start(start_tag));
                        out.push(Event::Text(snapshot.into()));
                        out.push(if is_image {
                            Event::End(TagEnd::Image)
                        } else {
                            Event::End(TagEnd::Link)
                        });
                    }
                }
            }
            continue;
        }

        match event {
            // 源文原始 HTML 降级为纯文本（下游 push_html 自动转义），保证注入安全
            Event::Html(raw) | Event::InlineHtml(raw) if opts.demote_raw_html => {
                out.push(Event::Text(raw))
            }
            Event::Start(tag) => {
                let dest = match &tag {
                    Tag::Link { dest_url, .. } => Some(dest_url.to_string()),
                    Tag::Image { dest_url, .. } => Some(dest_url.to_string()),
                    _ => None,
                };
                if let Some(dest) = dest {
                    let is_image = matches!(tag, Tag::Image { .. });
                    if let Some(r) = parse_ref_dest(&dest, registry) {
                        pending = Some((r, is_image, tag, String::new()));
                        continue;
                    }
                }
                out.push(Event::Start(tag));
            }
            other => out.push(other),
        }
    }

    // 异常兜底：链接未闭合时（理论上不会发生）至少把已收集的内容吐出来
    if let Some((r, is_image, start_tag, name_buf)) = pending.take() {
        let snapshot = normalize_snapshot(&name_buf, &r.id);
        if let Some(ev) = render(&r, &snapshot, is_image) {
            out.push(ev);
        } else {
            out.push(Event::Start(start_tag));
            out.push(Event::Text(snapshot.into()));
            out.push(if is_image {
                Event::End(TagEnd::Image)
            } else {
                Event::End(TagEnd::Link)
            });
        }
    }

    out
}
