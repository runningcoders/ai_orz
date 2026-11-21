//! Markdown 渲染中间表示（IR）与渠道无关计算（前后端共用）
//!
//! ## 分层
//!
//! - **本模块（common）**：`Event` 流 → 渠道无关 IR [`Block`]/[`Inline`]，
//!   以及三个纯函数——富度判定 [`richness`]、纯文本降级 [`blocks_to_plain_text`]、
//!   Markdown 回写 [`blocks_to_markdown`]。前端预览与后端出站共用同一套解析口径；
//! - **渠道 DAO**：拿通用能力按自身协议落地，协议独有结构不进 common。
//!   飞书出站已改用官方 `post` + `md` 标签直接承载 markdown 原文（原生支持
//!   标题 / 列表 / 表格 / 任务列表等全 GFM），不再依赖本模块的富度分形态。
//!
//! ## 富度分级
//!
//! [`Richness::Plain`] 纯文本；[`Richness::Inline`] 含行内样式 / 链接；
//! [`Richness::Block`] 含标题 / 列表 / 代码块 / 引用 / 分割线。该分级供需要
//! 按能力分层渲染的渠道 / 前端预览使用；飞书 md 标签全支持，无需此分级。
//! 表格在本 IR 中降级为普通文本段落（无跨渠道表格节点），但飞书走原文透传
//! 时表格由 md 标签原生渲染，不经过此 IR。

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

/// 行内节点（渠道无关）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// 普通文本（含软 / 硬换行归并）
    Text(String),
    /// 加粗
    Strong(Vec<Inline>),
    /// 斜体
    Emphasis(Vec<Inline>),
    /// 删除线
    Strikethrough(Vec<Inline>),
    /// 行内代码
    Code(String),
    /// 链接（含文本节点与原始 dest）
    Link {
        /// 链接展示文本
        text: Vec<Inline>,
        /// 原始目标（可能是 `attachment:` 等非 http scheme，由渲染器决定降级）
        dest: String,
    },
}

/// 列表项（扁平化，`depth` 保留嵌套层级供卡片缩进还原）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 嵌套深度（0 = 顶层）
    pub depth: usize,
    /// 项内行内节点
    pub inlines: Vec<Inline>,
}

/// 块节点（渠道无关）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// 段落
    Paragraph(Vec<Inline>),
    /// 标题（`level` 1~6）
    Heading {
        /// 标题级别
        level: u8,
        /// 标题行内节点
        inlines: Vec<Inline>,
    },
    /// 列表（有序 / 无序，项扁平带 depth）
    List {
        /// 是否有序
        ordered: bool,
        /// 列表项
        items: Vec<ListItem>,
    },
    /// 代码块
    CodeBlock {
        /// 语言标注（围栏信息串）
        lang: Option<String>,
        /// 代码正文（已含换行）
        code: String,
    },
    /// 引用（内部段落展平为行内节点序列）
    BlockQuote(Vec<Inline>),
    /// 分割线
    Rule,
}

/// 富度分级：决定渠道用什么容器承载
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Richness {
    /// 纯文本段落，无样式无块结构
    Plain,
    /// 含行内样式 / 链接，post 富文本可表达
    Inline,
    /// 含块级结构，post 无对应节点，需卡片
    Block,
}

/// 计算 IR 的富度分级（纯函数）
///
/// 块级节点（标题 / 列表 / 代码块 / 引用 / 分割线）→ [`Richness::Block`]；
/// 段落内含非 [`Inline::Text`] 节点 → [`Richness::Inline`]；否则 [`Richness::Plain`]。
pub fn richness(blocks: &[Block]) -> Richness {
    let mut level = Richness::Plain;
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                if inlines.iter().any(|n| !matches!(n, Inline::Text(_))) {
                    level = level.max(Richness::Inline);
                }
            }
            Block::Heading { .. }
            | Block::List { .. }
            | Block::CodeBlock { .. }
            | Block::BlockQuote(_)
            | Block::Rule => return Richness::Block,
        }
    }
    level
}

// ==================== 事件流 → IR ====================

/// 行内样式栈元素（`usize` = 进入样式时当前帧的 inline 起点，End 时整段包裹）
#[derive(Clone)]
enum Style {
    Strong(usize),
    Emphasis(usize),
    Strikethrough(usize),
    Link(String, usize),
}

/// 样式种类（由事件映射，push 时记录起点）
enum StyleKind {
    Strong,
    Emphasis,
    Strikethrough,
    Link(String),
}

/// 块构造帧
enum Frame {
    Paragraph {
        inlines: Vec<Inline>,
        styles: Vec<Style>,
    },
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        styles: Vec<Style>,
    },
    Item {
        depth: usize,
        inlines: Vec<Inline>,
        styles: Vec<Style>,
    },
    Quote {
        inlines: Vec<Inline>,
        styles: Vec<Style>,
    },
    Code {
        lang: Option<String>,
        code: String,
    },
}

impl Frame {
    fn inlines_mut(&mut self) -> Option<(&mut Vec<Inline>, &mut Vec<Style>)> {
        match self {
            Self::Paragraph { inlines, styles }
            | Self::Heading {
                inlines, styles, ..
            }
            | Self::Item {
                inlines, styles, ..
            }
            | Self::Quote { inlines, styles } => Some((inlines, styles)),
            Self::Code { .. } => None,
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// `Event` 流 → 渠道无关 [`Block`] 序列（纯函数）
///
/// HTML / InlineHtml 事件统一降级为文本（防注入）；表格结构标记被忽略，单元格
/// 文本按普通段落收集（表格无跨渠道统一节点）。
pub fn parse_events<'a>(events: impl IntoIterator<Item = Event<'a>>) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut root_inlines: Vec<Inline> = Vec::new();
    // 列表上下文栈：(ordered, depth)；遇到 Item 时取栈顶
    let mut lists: Vec<(bool, usize)> = Vec::new();

    for event in events {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => stack.push(Frame::Paragraph {
                    inlines: Vec::new(),
                    styles: Vec::new(),
                }),
                Tag::Heading { level, .. } => stack.push(Frame::Heading {
                    level: heading_level(level),
                    inlines: Vec::new(),
                    styles: Vec::new(),
                }),
                Tag::List(start) => {
                    // 嵌套列表开始前，父 Item 的 End 尚未到达；先结算父 Item，
                    // 保证「父项在前、子项在后」的 IR 顺序（刷 UI 不跳行）。
                    // match 而非「先 last 检查再 pop」：避免 clippy collapsible_if，
                    // 且栈顶非 Item 时不误吞其他帧。
                    match stack.pop() {
                        Some(Frame::Item { depth, inlines, .. }) => {
                            let ordered = lists.last().map(|(o, _)| *o).unwrap_or(false);
                            append_list_item(&mut blocks, ordered, depth, inlines);
                        }
                        // 非 Item 帧原样放回（嵌套列表前的其他结构不能丢）
                        other => {
                            if let Some(frame) = other {
                                stack.push(frame);
                            }
                        }
                    }
                    let depth = lists.len();
                    lists.push((start.is_some(), depth));
                }
                Tag::Item => {
                    let (_, depth) = lists.last().copied().unwrap_or((false, 0));
                    stack.push(Frame::Item {
                        depth,
                        inlines: Vec::new(),
                        styles: Vec::new(),
                    });
                }
                Tag::BlockQuote(_) => stack.push(Frame::Quote {
                    inlines: Vec::new(),
                    styles: Vec::new(),
                }),
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(info) => {
                            let info = info.trim().to_string();
                            (!info.is_empty()).then_some(info)
                        }
                        CodeBlockKind::Indented => None,
                    };
                    stack.push(Frame::Code {
                        lang,
                        code: String::new(),
                    });
                }
                Tag::Strong => push_style(&mut stack, StyleKind::Strong),
                Tag::Emphasis => push_style(&mut stack, StyleKind::Emphasis),
                Tag::Strikethrough => push_style(&mut stack, StyleKind::Strikethrough),
                Tag::Link { dest_url, .. } => {
                    push_style(&mut stack, StyleKind::Link(dest_url.to_string()))
                }
                // Image / Table* 等结构标记忽略，内部 Text 由下方分支收集
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(Frame::Paragraph { inlines, .. }) = stack.pop() {
                        // 引用内的段落并入 Quote（不产生独立 Paragraph 块）
                        let mut inlines = Some(inlines);
                        if let Some(Frame::Quote {
                            inlines: quote_inlines,
                            ..
                        }) = stack.last_mut()
                        {
                            let owned = inlines.take().expect("inlines 尚未被取用");
                            if !quote_inlines.is_empty() {
                                quote_inlines.push(Inline::Text("\n".to_string()));
                            }
                            quote_inlines.extend(owned);
                        }
                        if let Some(owned) = inlines {
                            blocks.push(Block::Paragraph(owned));
                        }
                    }
                }
                TagEnd::Heading(_) => {
                    if let Some(Frame::Heading { level, inlines, .. }) = stack.pop() {
                        blocks.push(Block::Heading { level, inlines });
                    }
                }
                TagEnd::Item => {
                    // 栈顶必须是 Item 才 pop；嵌套列表提前结算父 Item 后，引用等
                    // 其他帧可能位于栈顶，无条件 pop 会误吞并丢弃其已收集文本。
                    match stack.pop() {
                        Some(Frame::Item { depth, inlines, .. }) => {
                            let ordered = lists.last().map(|(o, _)| *o).unwrap_or(false);
                            append_list_item(&mut blocks, ordered, depth, inlines);
                        }
                        other => {
                            if let Some(frame) = other {
                                stack.push(frame);
                            }
                        }
                    }
                }
                TagEnd::List(_) => {
                    lists.pop();
                }
                TagEnd::BlockQuote(_) => {
                    if let Some(Frame::Quote { inlines, .. }) = stack.pop() {
                        blocks.push(Block::BlockQuote(inlines));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some(Frame::Code { lang, code }) = stack.pop() {
                        blocks.push(Block::CodeBlock {
                            lang,
                            code: code.trim_end_matches('\n').to_string(),
                        });
                    }
                }
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                    pop_style(&mut stack);
                }
                _ => {}
            },
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                let text = text.to_string();
                push_text(&mut stack, &mut root_inlines, text);
            }
            Event::SoftBreak | Event::HardBreak => {
                push_text(&mut stack, &mut root_inlines, "\n".to_string());
            }
            Event::Rule => blocks.push(Block::Rule),
            // FootnoteReference / TaskListMarker 等低级事件无跨渠道语义，忽略
            _ => {}
        }
    }

    // 兜底：解析结束时未闭合的帧按已收集内容吐出（异常输入不吞文本）
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Paragraph { inlines, .. } => blocks.push(Block::Paragraph(inlines)),
            Frame::Heading { level, inlines, .. } => blocks.push(Block::Heading { level, inlines }),
            Frame::Item { depth, inlines, .. } => {
                let ordered = lists.last().map(|(o, _)| *o).unwrap_or(false);
                append_list_item(&mut blocks, ordered, depth, inlines);
            }
            Frame::Quote { inlines, .. } => blocks.push(Block::BlockQuote(inlines)),
            Frame::Code { lang, code } => blocks.push(Block::CodeBlock { lang, code }),
        }
    }
    if !root_inlines.is_empty() {
        blocks.push(Block::Paragraph(root_inlines));
    }
    blocks
}

fn push_style(stack: &mut [Frame], kind: StyleKind) {
    if let Some((inlines, styles)) = stack.last_mut().and_then(Frame::inlines_mut) {
        let pos = inlines.len();
        styles.push(match kind {
            StyleKind::Strong => Style::Strong(pos),
            StyleKind::Emphasis => Style::Emphasis(pos),
            StyleKind::Strikethrough => Style::Strikethrough(pos),
            StyleKind::Link(dest) => Style::Link(dest, pos),
        });
    }
}

/// End 标签时把该样式覆盖的 inline 段整体包裹成一个节点
/// （而非逐 Text 包裹，否则 `[**粗**链](url)` 会被拆成两个 Link）
fn pop_style(stack: &mut [Frame]) {
    let Some((inlines, styles)) = stack.last_mut().and_then(Frame::inlines_mut) else {
        return;
    };
    let Some(style) = styles.pop() else {
        return;
    };
    let wrapped = match style {
        Style::Strong(pos) => Inline::Strong(inlines.split_off(pos)),
        Style::Emphasis(pos) => Inline::Emphasis(inlines.split_off(pos)),
        Style::Strikethrough(pos) => Inline::Strikethrough(inlines.split_off(pos)),
        Style::Link(dest, pos) => Inline::Link {
            text: inlines.split_off(pos),
            dest,
        },
    };
    inlines.push(wrapped);
}

fn push_text(stack: &mut [Frame], root: &mut Vec<Inline>, text: String) {
    if text.is_empty() {
        return;
    }
    match stack.last_mut() {
        // 代码块帧：文本直接进 code（不走行内样式包裹）
        Some(Frame::Code { code, .. }) => code.push_str(&text),
        Some(frame) => {
            if let Some((inlines, _)) = frame.inlines_mut() {
                inlines.push(Inline::Text(text));
            }
        }
        None => root.push(Inline::Text(text)),
    }
}

fn append_list_item(blocks: &mut Vec<Block>, ordered: bool, depth: usize, inlines: Vec<Inline>) {
    // 相邻同序 Item 合并进同一个 List（depth 保留在项内，不改 List 边界）
    let can_merge = matches!(
        blocks.last(),
        Some(Block::List {
            ordered: prev_ordered,
            ..
        }) if *prev_ordered == ordered
    );
    let item = ListItem { depth, inlines };
    if can_merge {
        let Some(Block::List { items, .. }) = blocks.last_mut() else {
            unreachable!("can_merge 已保证末块为 List");
        };
        items.push(item);
    } else {
        blocks.push(Block::List {
            ordered,
            items: vec![item],
        });
    }
}

// ==================== IR → 展示形态 ====================

/// IR → 纯文本（外部纯文本渠道兜底）
///
/// 结构标记不还原，段落 / 标题 / 列表项 / 引用 / 代码块边界补换行，
/// 行内节点递归取文本，末尾 trim。
pub fn blocks_to_plain_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                inline_texts(inlines, &mut out);
                out.push('\n');
            }
            Block::List { items, .. } => {
                for item in items {
                    inline_texts(&item.inlines, &mut out);
                    out.push('\n');
                }
            }
            Block::BlockQuote(inlines) => {
                inline_texts(inlines, &mut out);
                out.push('\n');
            }
            Block::CodeBlock { code, .. } => {
                out.push_str(code);
                out.push('\n');
            }
            Block::Rule => {}
        }
    }
    out.trim().to_string()
}

fn inline_texts(inlines: &[Inline], out: &mut String) {
    for node in inlines {
        match node {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(n) | Inline::Emphasis(n) | Inline::Strikethrough(n) => {
                inline_texts(n, out)
            }
            Inline::Link { text, .. } => inline_texts(text, out),
        }
    }
}

/// IR → Markdown 源文（卡片 `markdown` 组件等需要原文的渠道用）
///
/// 从 IR 回写而非直接用原文：raw HTML 已在 [`parse_events`] 降级为文本，
/// 非 http(s) 协议链接由调用方在此结果上再做兜底（渲染器职责）。
pub fn blocks_to_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                inline_markdown(inlines, &mut out);
                out.push_str("\n\n");
            }
            Block::Heading { level, inlines } => {
                for _ in 0..*level {
                    out.push('#');
                }
                out.push(' ');
                inline_markdown(inlines, &mut out);
                out.push_str("\n\n");
            }
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    for _ in 0..item.depth {
                        out.push_str("    ");
                    }
                    if *ordered {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    inline_markdown(&item.inlines, &mut out);
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::CodeBlock { lang, code } => {
                out.push_str("```");
                if let Some(lang) = lang {
                    out.push_str(lang);
                }
                out.push('\n');
                out.push_str(code);
                out.push_str("\n```\n\n");
            }
            Block::BlockQuote(inlines) => {
                out.push_str("> ");
                inline_markdown(inlines, &mut out);
                out.push_str("\n\n");
            }
            Block::Rule => out.push_str("---\n\n"),
        }
    }
    out.trim().to_string()
}

fn inline_markdown(inlines: &[Inline], out: &mut String) {
    for node in inlines {
        match node {
            Inline::Text(t) => out.push_str(t),
            Inline::Strong(n) => {
                out.push_str("**");
                inline_markdown(n, out);
                out.push_str("**");
            }
            Inline::Emphasis(n) => {
                out.push('*');
                inline_markdown(n, out);
                out.push('*');
            }
            Inline::Strikethrough(n) => {
                out.push_str("~~");
                inline_markdown(n, out);
                out.push_str("~~");
            }
            Inline::Code(t) => {
                // 内容含反引号时用更长的围栏，避免 markdown 结构破损
                let fence = "`".repeat(t.matches('`').count() + 1);
                out.push_str(&fence);
                out.push_str(t);
                out.push_str(&fence);
            }
            Inline::Link { text, dest } => {
                out.push('[');
                inline_markdown(text, out);
                out.push_str("](");
                // URL 含空格或括号时用 <> 包裹（CommonMark 标准转义）
                if dest.contains(char::is_whitespace) || dest.contains('(') || dest.contains(')') {
                    out.push('<');
                    out.push_str(dest);
                    out.push('>');
                } else {
                    out.push_str(dest);
                }
                out.push(')');
            }
        }
    }
}

#[cfg(test)]
#[path = "markdown_render_tests.rs"]
mod tests;
