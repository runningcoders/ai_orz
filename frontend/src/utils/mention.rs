//! 消息 @ 提及：文本协议解析与渲染
//!
//! ## 协议
//!
//! 采用标准 CommonMark 链接语法承载提及，dest 为 `type:id`：
//!
//! ```text
//! [@张伟](agent:agt_7f3)
//! [@数据清洗](task:tsk_a91)
//! [@客户数据平台](project:prj_2c8)
//! ```
//!
//! 选它而非自定义语法（如 `<@agent:agt_7f3>`）的关键理由是**降级安全**：
//! 它本身是合法 CommonMark，渲染器未识别时退化成一个普通链接，
//! 而不会把 `agent:agt_7f3` 这类原始串直接暴露给用户。
//! pulldown-cmark 零配置即可解析，不需要自定义扩展。
//!
//! ## 为什么用文本协议而不是独立的 mentions 列
//!
//! - **消息体自包含**：复制 / 转发 / 导入导出都不丢提及信息，历史消息无需返查
//! - **零存储改动**：不需要 migration，也就不会触发 sqlx 离线缓存（`.sqlx`）重生成
//! - **Agent 侧同样可发**：Agent 在回复正文里写语法即可，无需为它新增工具参数
//! - **名字不过期**：链接文本只是快照回退值，渲染时优先用 Directory 实时名覆盖
//!
//! ## 安全
//!
//! chip 的 HTML 由本模块生成，而展示名来自消息正文（用户可控），
//! 拼接前一律 [`escape_html`]，避免经 `dangerous_inner_html` 注入 XSS。

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::utils::message::NameMap;

/// 提及实体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionKind {
    /// 组织内 Agent
    Agent,
    /// 任务
    Task,
    /// 项目
    Project,
}

impl MentionKind {
    /// 协议前缀（`type:id` 的 type 部分）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Task => "task",
            Self::Project => "project",
        }
    }

    /// chip 的配色类后缀（样式见 `styles/input.css`）
    pub fn chip_class(self) -> &'static str {
        match self {
            Self::Agent => "mention-agent",
            Self::Task => "mention-task",
            Self::Project => "mention-project",
        }
    }
}

impl std::str::FromStr for MentionKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "agent" => Ok(Self::Agent),
            "task" => Ok(Self::Task),
            "project" => Ok(Self::Project),
            _ => Err(()),
        }
    }
}

/// 从消息文本里解析出的一个提及
#[derive(Debug, Clone, PartialEq)]
pub struct MentionRef {
    /// 实体类型
    pub kind: MentionKind,
    /// 实体 ID
    pub id: String,
}

/// 解析 Markdown 链接 dest 为提及（`agent:agt_7f3`）
///
/// 非提及协议（http / 站内相对路径 / mailto 等）一律返回 `None`，
/// 交回常规链接渲染流程，不干扰文档中心的站内链接处理。
pub fn parse_mention_dest(dest: &str) -> Option<MentionRef> {
    let (kind_part, id) = dest.split_once(':')?;
    let kind: MentionKind = kind_part.parse().ok()?;
    // 空 id 或含空白的 dest 不视为提及：既防脏数据，也避免误吞普通链接
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }
    Some(MentionRef {
        kind,
        id: id.to_string(),
    })
}

/// 生成提及语法文本（输入框插入 / 发送前拼装用）
///
/// `name` 仅作为展示快照写入链接文本，渲染时优先被 Directory 实时名覆盖。
/// 名字里的 `[` `]` `\` 会破坏 Markdown 链接语法，这里做最小转义。
pub fn format_mention(kind: MentionKind, id: &str, name: &str) -> String {
    let safe_name = name
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    format!("[@{}]({}:{})", safe_name, kind.as_str(), id)
}

/// @ 查询词最大字节长度（超出则不再视为激活的提及查询）
const MAX_QUERY_BYTES: usize = 32;

/// 输入框内一个处于激活状态的 @ 查询
///
/// 由 [`detect_mention_query`] 从「文本 + 光标位置」推导，是插入替换的作用域。
#[derive(Debug, Clone, PartialEq)]
pub struct MentionQuery {
    /// `@` 在文本中的字节下标
    pub start: usize,
    /// 光标字节下标，等于 `start + 1 + query.len()`
    pub caret: usize,
    /// `@` 之后已输入的搜索词（不含空白）
    pub query: String,
}

/// `@` 前一个字符是否属于可触发边界
///
/// 行首 / 空白 / 常见开括号可以触发；字母数字（如邮箱 `a@b.com`）不触发，
/// 避免把邮箱、URL 里的 @ 误判成提及。
fn is_mention_boundary(prev: Option<char>) -> bool {
    matches!(
        prev,
        None | Some(' ')
            | Some('\n')
            | Some('\t')
            | Some('(')
            | Some('（')
            | Some('[')
            | Some('【')
            | Some('{')
    )
}

/// 查询词里一旦出现这些字符，说明光标已不在「刚打完 @ 关键词」的位置
///
/// 典型反例：刚插入的 `[@张伟](agent:agt_1)` 光标停在末尾时，
/// 从右往左找 `@` 会得到 `张伟](agent:agt_1)`——必须靠这个黑名单挡掉，
/// 否则每次插入后菜单都会立刻重新弹出。
fn is_query_char(c: char) -> bool {
    !c.is_whitespace()
        && !matches!(
            c,
            '(' | ')' | '[' | ']' | '{' | '}' | '（' | '）' | '【' | '】'
        )
}

/// 检测光标处是否处于「正在输入 @ 查询」的状态
///
/// 纯函数、不依赖 DOM：`caret` 由调用方从 textarea 的 `selectionStart` 读取后传入。
/// 返回 `Some` 时表示应弹出候选菜单，选中后按 `[start..caret]` 区间做替换。
pub fn detect_mention_query(text: &str, caret: usize) -> Option<MentionQuery> {
    if caret == 0 || caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    // 取光标前最近的一个 @：用户连续输入时生效的永远是最后一个
    let start = before.rfind('@')?;
    let query = &before[start + 1..];
    if query.len() > MAX_QUERY_BYTES || !query.chars().all(is_query_char) {
        return None;
    }
    if !is_mention_boundary(before[..start].chars().next_back()) {
        return None;
    }
    Some(MentionQuery {
        start,
        caret,
        query: query.to_string(),
    })
}

/// 把选中的提及写入文本：替换 `[start..caret]`，并在末尾补一个空格
///
/// 返回 `(新文本, 新光标位置)`，调用方需把光标设回新位置，
/// 否则受控 textarea 重渲染后光标会跳到末尾。
pub fn apply_mention_pick(text: &str, q: &MentionQuery, token: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len() + token.len() + 1);
    out.push_str(&text[..q.start]);
    out.push_str(token);
    out.push(' ');
    out.push_str(&text[q.caret..]);
    let caret = q.start + token.len() + 1;
    (out, caret)
}

/// 从文本里摘掉一个已插入的提及 token（「已提及」胶囊删除时用）
///
/// 连同其后紧跟的一个空格一起删除，避免在正文里留下空洞。
pub fn remove_mention_token(text: &str, token: &str) -> String {
    match text.find(token) {
        Some(pos) => {
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..pos]);
            let rest = &text[pos + token.len()..];
            // 只吃紧跟其后的一个空格，保留用户后续输入的内容
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            out.push_str(rest);
            out
        }
        // token 已被用户手动编辑掉：文本以当前值为准，不做任何改动
        None => text.to_string(),
    }
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
pub fn render_mention_chip(m: &MentionRef, display_name: &str) -> String {
    let kind = m.kind.as_str();
    let cls = m.kind.chip_class();
    let id = escape_html(&m.id);
    let name = escape_html(display_name);
    format!(
        r#"<span class="mention-chip {cls}" data-mention-kind="{kind}" data-mention-id="{id}" title="{kind}: {id}">@{name}</span>"#
    )
}

/// chip 展示名：Directory 实时名优先，回退到文本里的快照名
///
/// 目前只有 Agent 名有全局目录（`store::directory`）可查；
/// 任务 / 项目名称没有全局缓存，直接用文本快照，避免为了渲染发起请求。
pub fn resolve_display_name(m: &MentionRef, snapshot: &str, agents: Option<&NameMap>) -> String {
    // 三个条件同时满足才升级为实时名：Agent 类型 + 目录命中 + 名字非空
    if m.kind == MentionKind::Agent
        && let Some(name) = agents.and_then(|map| map.get(&m.id))
        && !name.trim().is_empty()
    {
        return name.clone();
    }
    snapshot.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mention_dest_accepts_three_kinds() {
        assert_eq!(
            parse_mention_dest("agent:agt_7f3"),
            Some(MentionRef {
                kind: MentionKind::Agent,
                id: "agt_7f3".to_string()
            })
        );
        assert_eq!(
            parse_mention_dest("task:tsk_a91"),
            Some(MentionRef {
                kind: MentionKind::Task,
                id: "tsk_a91".to_string()
            })
        );
        assert_eq!(
            parse_mention_dest("project:prj_2c8"),
            Some(MentionRef {
                kind: MentionKind::Project,
                id: "prj_2c8".to_string()
            })
        );
    }

    #[test]
    fn parse_mention_dest_rejects_non_mention() {
        // 普通链接 / 站内链接 / 邮链不得被误吞
        assert_eq!(parse_mention_dest("https://example.com/a"), None);
        assert_eq!(parse_mention_dest("docs/design/runtime.md"), None);
        assert_eq!(parse_mention_dest("mailto:a@b.com"), None);
        // 未知类型与空 id
        assert_eq!(parse_mention_dest("user:u_1"), None);
        assert_eq!(parse_mention_dest("agent:"), None);
        // 含空白（原文形如 [x](agent:a b)）
        assert_eq!(parse_mention_dest("agent:a b"), None);
    }

    #[test]
    fn format_mention_escapes_bracket() {
        assert_eq!(
            format_mention(MentionKind::Agent, "agt_1", "张伟"),
            "[@张伟](agent:agt_1)"
        );
        // 名字里的 ] 会截断链接语法，必须转义
        assert_eq!(
            format_mention(MentionKind::Task, "tsk_1", "a]b"),
            "[@a\\]b](task:tsk_1)"
        );
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
    fn resolve_display_name_prefers_directory() {
        let mut agents = NameMap::new();
        agents.insert("agt_7f3".to_string(), "张伟（新）".to_string());

        let m = MentionRef {
            kind: MentionKind::Agent,
            id: "agt_7f3".to_string(),
        };
        // 命中目录：用实时名，快照名被覆盖（改名后历史消息自动同步）
        assert_eq!(
            resolve_display_name(&m, "张伟", Some(&agents)),
            "张伟（新）"
        );
        // 未命中：回退快照
        assert_eq!(resolve_display_name(&m, "张伟", None), "张伟");

        // Task 类型不走 Agent 目录
        let t = MentionRef {
            kind: MentionKind::Task,
            id: "tsk_a91".to_string(),
        };
        assert_eq!(
            resolve_display_name(&t, "数据清洗", Some(&agents)),
            "数据清洗"
        );
    }

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

    /// 便捷构造：`text` 里用 `|` 标记光标位置
    fn detect(text: &str) -> Option<MentionQuery> {
        let caret = text.find('|').expect("测试文本需用 | 标记光标");
        let clean = text.replace('|', "");
        detect_mention_query(&clean, caret)
    }

    #[test]
    fn detect_query_triggers_on_boundary() {
        // 行首刚打完 @：菜单应打开，关键词为空（展示全部候选）
        let q = detect("@|").expect("行首 @ 应触发");
        assert_eq!((q.start, q.caret, q.query.as_str()), (0, 1, ""));

        // 空格后 @ + 中文关键词
        let q = detect("你好 @张|").expect("空格后 @ 应触发");
        assert_eq!((q.start, q.query.as_str()), (7, "张"));

        // 开括号后同样触发（用户刚删掉一个提及再重新 @ 的常见位置）
        assert!(detect("（@|").is_some());
    }

    #[test]
    fn detect_query_rejects_email_and_plain_text() {
        // 邮箱 / 非边界字符后的 @ 不得误触发
        assert!(detect("a@b.com|").is_none());
        assert!(detect("联系zhang@|").is_none());
        // 光标不在 @ 之后
        assert!(detect("@张伟 |").is_none());
        // 纯文本无 @
        assert!(detect("你好世界|").is_none());
    }

    #[test]
    fn detect_query_rejects_after_inserted_mention() {
        // 关键回归：刚插入的提及语法里含 @，光标停在末尾时不得再次弹菜单
        assert!(detect("[@张伟](agent:agt_1)|").is_none());
        // 提及后再输入普通文字，同样不该弹
        assert!(detect("[@张伟](agent:agt_1) 你好|").is_none());
        // 但空格后重新打 @ 应当正常触发
        let q = detect("[@张伟](agent:agt_1) @李|").expect("新 @ 应触发");
        assert_eq!(q.query, "李");
    }

    #[test]
    fn apply_pick_replaces_query_range() {
        let q = detect("看下 @张| 的进度").expect("应检测到查询");
        let token = format_mention(MentionKind::Agent, "agt_7f3", "张伟");
        let (text, caret) = apply_mention_pick("看下 @张 的进度", &q, &token);

        // 替换区间是 @ 起至光标位，原文里被 @ 查询占掉的「张」一并吃掉
        assert_eq!(text, "看下 [@张伟](agent:agt_7f3)  的进度");
        // 光标停在插入内容之后（含补的空格）
        assert_eq!(caret, q.start + token.len() + 1);
        assert_eq!(&text[..caret], "看下 [@张伟](agent:agt_7f3) ");
    }

    #[test]
    fn apply_pick_at_text_end() {
        let q = detect("你好 @|").expect("应检测到查询");
        let token = format_mention(MentionKind::Task, "tsk_a91", "数据清洗");
        let (text, caret) = apply_mention_pick("你好 @", &q, &token);
        assert_eq!(text, "你好 [@数据清洗](task:tsk_a91) ");
        assert_eq!(caret, text.len());
    }

    #[test]
    fn remove_token_strips_one_space() {
        let token = format_mention(MentionKind::Agent, "agt_1", "张伟");
        let text = format!("请 {} 跟进一下", token);
        assert_eq!(remove_mention_token(&text, &token), "请 跟进一下");
        // token 不存在时原样返回，不破坏用户已编辑的内容
        assert_eq!(remove_mention_token("随便写的", &token), "随便写的");
        // 尾部提及（后面没有空格）也能干净摘除
        let tail = format!("你好 {}", token);
        assert_eq!(remove_mention_token(&tail, &token), "你好 ");
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
}
