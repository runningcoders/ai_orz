//! 消息 @ 提及：文本协议核心（前后端共享的单一事实源）
//!
//! ## 协议
//!
//! 采用标准 CommonMark 链接语法承载提及，dest 为 `type:id`：
//!
//! ```text
//! [@张伟](agent:agt_7f3)
//! [@远端助手](agent:agt_9@org-B12)   ← 跨组织 Agent（org_id 为对端组织 ID）
//! [@数据清洗](task:tsk_a91)
//! [@客户数据平台](project:prj_2c8)
//! ```
//!
//! 选它而非自定义语法（如 `<@agent:agt_7f3>`）的关键理由是**降级安全**：
//! 它本身是合法 CommonMark，渲染器未识别时退化成一个普通链接，
//! 而不会把 `agent:agt_7f3` 这类原始串直接暴露给用户。
//!
//! ## 分层
//!
//! 本模块是**纯文本协议层**：解析、拼装、输入检测、提及提取，不依赖任何
//! 渲染库或 DOM。前端（`frontend/src/utils/mention.rs`）在此之上叠加
//! pulldown-cmark 渲染与 chip HTML；后端（如 prompt 注入）直接用
//! [`extract_mentions`] 从消息正文提取提及，无需自己实现解析。
//! 协议迭代只改本模块，前后端天然一致。
//!
//! ## 为什么用文本协议而不是独立的 mentions 列
//!
//! - **消息体自包含**：复制 / 转发 / 导入导出都不丢提及信息，历史消息无需返查
//! - **零存储改动**：不需要 migration，也就不会触发 sqlx 离线缓存（`.sqlx`）重生成
//! - **Agent 侧同样可发**：Agent 在回复正文里写语法即可，无需为它新增工具参数
//! - **名字不过期**：链接文本只是快照回退值，渲染时优先用实时名覆盖

use std::collections::HashMap;

/// 提及实体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// chip 的配色类后缀（前端样式类 `mention-{后缀}`，见 `styles/input.css`）
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MentionRef {
    /// 实体类型
    pub kind: MentionKind,
    /// 实体 ID
    pub id: String,
    /// 跨组织限定（仅 Agent 提及）：`agent:<id>@<org_id>` 中的 org_id，
    /// 即对端组织 ID。None = 组织内寻址。
    /// 合法性（org 是否有 Active 连接）由上层校验，协议层只做格式解析。
    #[serde(default)]
    pub org: Option<String>,
}

/// 解析后的提及（已带可读名 + 可选上下文摘要），供后端 prompt 注入等消费场景
///
/// 与 [`MentionRef`] 的区别：[`MentionRef`] 只含 `kind + id`（协议层提取结果），
/// 本结构额外携带调用方借助 DAL / 目录解析出的可读 `name` 与一行 `summary`。
/// 本结构自身不依赖任何存储层，可前后端共享。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMention {
    /// 实体类型
    pub kind: MentionKind,
    /// 实体 ID
    pub id: String,
    /// 可读展示名（已解析：实时目录命中或正文快照回退）
    pub name: String,
    /// 一行上下文摘要（可选）：任务状态、Agent 角色、项目描述等
    pub summary: Option<String>,
}

impl ResolvedMention {
    /// 类型中文标签（prompt 区块与前端 chip 共用）
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            MentionKind::Agent => "Agent",
            MentionKind::Task => "任务",
            MentionKind::Project => "项目",
        }
    }
}

/// 解析 Markdown 链接 dest 为提及（`agent:agt_7f3` / `agent:agt_9@org-B12`）
///
/// 非提及协议（http / 站内相对路径 / mailto 等）一律返回 `None`，
/// 交回常规链接渲染流程，不干扰文档中心的站内链接处理。
/// 跨组织形态（`agent:<id>@<org_id>`）仅 Agent 支持；格式不合法
/// （空段 / 多个 @ / 非 Agent 类型带 @）同样返回 None，降级普通链接。
pub fn parse_mention_dest(dest: &str) -> Option<MentionRef> {
    let (kind_part, id) = dest.split_once(':')?;
    let kind: MentionKind = kind_part.parse().ok()?;
    // 空 id 或含空白的 dest 不视为提及：既防脏数据，也避免误吞普通链接
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }
    let (id, org) = match id.split_once('@') {
        Some((agent_id, org_id)) if kind == MentionKind::Agent => {
            if agent_id.is_empty() || org_id.is_empty() || org_id.contains('@') {
                return None;
            }
            (agent_id, Some(org_id.to_string()))
        }
        // 非 Agent 类型不支持 org 后缀：按普通链接处理
        Some(_) => return None,
        None => (id, None),
    };
    Some(MentionRef {
        kind,
        id: id.to_string(),
        org,
    })
}

/// 生成提及语法文本（输入框插入 / 发送前拼装用）
///
/// `name` 仅作为展示快照写入链接文本，渲染时优先被实时名覆盖。
/// 名字里的 `[` `]` `\` 会破坏 Markdown 链接语法，这里做最小转义。
pub fn format_mention(kind: MentionKind, id: &str, name: &str) -> String {
    format_mention_ref(
        &MentionRef {
            kind,
            id: id.to_string(),
            org: None,
        },
        name,
    )
}

/// 生成提及语法文本（[`MentionRef`] 版）
///
/// 跨组织提及（`org = Some`）输出 `agent:<id>@<org_id>` dest；
/// 组织内提及与 [`format_mention`] 输出一致。
pub fn format_mention_ref(m: &MentionRef, name: &str) -> String {
    let safe_name = name
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let dest = match &m.org {
        Some(org) => format!("{}:{}@{}", m.kind.as_str(), m.id, org),
        None => format!("{}:{}", m.kind.as_str(), m.id),
    };
    format!("[@{}]({})", safe_name, dest)
}

/// @ 查询词最大字节长度（超出则不再视为激活的提及查询）
const MAX_QUERY_BYTES: usize = 32;

/// 输入框内一个处于激活状态的 @ 查询
///
/// 由 [`detect_mention_query`] 从「文本 + 光标位置」推导，是插入替换的作用域。
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// 从消息正文提取全部提及及其链接文本（快照名），后端 prompt 注入 / 通知场景用
///
/// 返回 `(MentionRef, 链接文本)`：链接文本即 `[@这里](type:id)` 里的「这里」，
/// 作为展示名快照回退值（Agent 名渲染时优先被实时目录覆盖，任务 / 项目名称
/// 无全局缓存时用它）。
///
/// 轻量扫描 `[...](type:id)` 链接语法，无需 Markdown 解析器：
/// 找到每个 `](` 后读取到下一个 `)` 为止的 dest，能解析成提及即收录。
/// dest 含空白会提前放弃（与 CommonMark 无尖括号 dest 规则一致），
/// 已转义的 `]`（`\]`）不视为链接闭合。
///
/// 注意：这是面向「Agent 写回的规整协议文本」的提取器，不追求完整
/// Markdown 语义；渲染场景请用前端基于 pulldown-cmark 事件流的实现。
pub fn extract_mentions_with_text(text: &str) -> Vec<(MentionRef, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("](") {
        let bracket = i + off;
        // 回溯找链接文本起点 `[`（取 `](` 之前最近的那个，排除孤立的 "](...)"）。
        // 真正的协议过滤由 parse_mention_dest 完成。
        let Some(open) = text[..bracket].rfind('[') else {
            i = bracket + 2;
            continue;
        };
        // 链接文本（展示快照）：剥掉前导 @（它是提及触发符，不是实体名本身）
        let text_snapshot = text[open + 1..bracket].trim_start_matches('@').to_string();
        // dest 起点固定在 bracket+2
        let dest_start = bracket + 2;
        let dest_end = text[dest_start..]
            .find(|c: char| c == ')' || c.is_whitespace())
            .map(|p| dest_start + p);
        let Some(dest_end) = dest_end else {
            i = dest_start;
            continue;
        };
        if bytes[dest_end] != b')' {
            // dest 含空白：不是合法链接，跳过
            i = dest_end + 1;
            continue;
        }
        if let Some(m) = parse_mention_dest(&text[dest_start..dest_end]) {
            out.push((m, text_snapshot));
        }
        i = dest_end + 1;
    }
    out
}

/// 从消息正文中提取全部提及（见 [`extract_mentions_with_text`]，本函数只取 kind+id）
///
/// 后端 prompt 注入时通常要连名字一起解析，请用 [`extract_mentions_with_text`]。
pub fn extract_mentions(text: &str) -> Vec<MentionRef> {
    extract_mentions_with_text(text)
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

/// chip 展示名：实时名优先，回退到文本里的快照名
///
/// `agents` 为 id → 展示名的映射（前端传 Directory 目录，后端传 Agent 表查询结果）。
/// 目前只有 Agent 名有全局目录可查；任务 / 项目名称没有全局缓存，
/// 直接用文本快照，避免为了渲染发起请求。
pub fn resolve_display_name(
    m: &MentionRef,
    snapshot: &str,
    agents: Option<&HashMap<String, String>>,
) -> String {
    // 三个条件同时满足才升级为实时名：Agent 类型 + 目录命中 + 名字非空
    if m.kind == MentionKind::Agent
        && let Some(name) = agents.and_then(|map| map.get(&m.id))
        && !name.trim().is_empty()
    {
        return name.clone();
    }
    snapshot.to_string()
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
                id: "agt_7f3".to_string(),
                org: None
            })
        );
        assert_eq!(
            parse_mention_dest("task:tsk_a91"),
            Some(MentionRef {
                kind: MentionKind::Task,
                id: "tsk_a91".to_string(),
                org: None
            })
        );
        assert_eq!(
            parse_mention_dest("project:prj_2c8"),
            Some(MentionRef {
                kind: MentionKind::Project,
                id: "prj_2c8".to_string(),
                org: None
            })
        );
    }

    #[test]
    fn parse_mention_dest_federated_agent() {
        assert_eq!(
            parse_mention_dest("agent:agt_9@org-B12"),
            Some(MentionRef {
                kind: MentionKind::Agent,
                id: "agt_9".to_string(),
                org: Some("org-B12".to_string())
            })
        );
        // 非法形态降级普通链接
        assert_eq!(parse_mention_dest("agent:@org-B12"), None);
        assert_eq!(parse_mention_dest("agent:agt_9@"), None);
        assert_eq!(parse_mention_dest("agent:agt_9@org@a"), None);
        // 非 Agent 类型不支持 org 后缀
        assert_eq!(parse_mention_dest("task:tsk_1@org-B12"), None);
        // org 段含空白整体不合法（外层空白检查已覆盖，这里验证组合形态）
        assert_eq!(parse_mention_dest("agent:agt_9@org B"), None);
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
    fn resolve_display_name_prefers_directory() {
        let mut agents = HashMap::new();
        agents.insert("agt_7f3".to_string(), "张伟（新）".to_string());

        let m = MentionRef {
            kind: MentionKind::Agent,
            id: "agt_7f3".to_string(),
            org: None,
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
            org: None,
        };
        assert_eq!(
            resolve_display_name(&t, "数据清洗", Some(&agents)),
            "数据清洗"
        );
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
    fn extract_finds_all_kinds_in_order() {
        let text = "请 [@张伟](agent:agt_1) 跟进 [@A](task:t1) 和 [@B](task:t2)，\
                    背景见 [@平台](project:p1)";
        assert_eq!(
            extract_mentions(text),
            vec![
                MentionRef {
                    kind: MentionKind::Agent,
                    id: "agt_1".into(),
                    org: None
                },
                MentionRef {
                    kind: MentionKind::Task,
                    id: "t1".into(),
                    org: None
                },
                MentionRef {
                    kind: MentionKind::Task,
                    id: "t2".into(),
                    org: None
                },
                MentionRef {
                    kind: MentionKind::Project,
                    id: "p1".into(),
                    org: None
                },
            ]
        );
    }

    #[test]
    fn extract_ignores_normal_links_and_emails() {
        let text = "[文档](https://example.com) 与 [站内](docs/a.md) 参考，\
                    邮箱 a@b.com 不含链接语法；[@张伟](agent:agt_1) 是唯一提及";
        assert_eq!(
            extract_mentions(text),
            vec![MentionRef {
                kind: MentionKind::Agent,
                id: "agt_1".into(),
                org: None
            }]
        );
    }

    #[test]
    fn extract_federated_agent_mention() {
        let text = "请 [@远端助手](agent:agt_9@org-B12) 帮忙翻译";
        let got = extract_mentions_with_text(text);
        assert_eq!(
            got,
            vec![(
                MentionRef {
                    kind: MentionKind::Agent,
                    id: "agt_9".into(),
                    org: Some("org-B12".into())
                },
                "远端助手".to_string()
            )]
        );
    }

    #[test]
    fn format_mention_ref_federated_roundtrip() {
        let m = parse_mention_dest("agent:agt_9@org-B12").unwrap();
        let token = format_mention_ref(&m, "远端助手");
        assert_eq!(token, "[@远端助手](agent:agt_9@org-B12)");
        // 序列化往返：旧数据（无 org 字段）反序列化为 None
        let legacy: MentionRef = serde_json::from_str(r#"{"kind":"Agent","id":"agt_1"}"#).unwrap();
        assert_eq!(legacy.org, None);
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            serde_json::json!({"kind":"Agent","id":"agt_9","org":"org-B12"})
        );
    }

    #[test]
    fn extract_skips_unclosed_and_whitespace_dest() {
        // 未闭合链接、dest 含空白都不是提及
        assert!(extract_mentions("[@x](agent:agt_1").is_empty());
        assert!(extract_mentions("[@x](agent:agt_1 more)").is_empty());
        assert!(extract_mentions("没有链接的普通文本").is_empty());
    }

    #[test]
    fn extract_survives_escaped_bracket_name() {
        // format_mention 转义过的名字（含 \]）仍能正常提取
        let token = format_mention(MentionKind::Agent, "agt_1", "a]b");
        assert_eq!(
            extract_mentions(&format!("看 {} ", token)),
            vec![MentionRef {
                kind: MentionKind::Agent,
                id: "agt_1".into(),
                org: None
            }]
        );
    }

    #[test]
    fn extract_with_text_captures_snapshot_name() {
        let text = "请 [@张伟](agent:agt_1) 看 [@数据清洗](task:tsk_a) 与 [@平台](project:prj_1)";
        let got = extract_mentions_with_text(text);
        assert_eq!(
            got,
            vec![
                (
                    MentionRef {
                        kind: MentionKind::Agent,
                        id: "agt_1".into(),
                        org: None
                    },
                    "张伟".to_string()
                ),
                (
                    MentionRef {
                        kind: MentionKind::Task,
                        id: "tsk_a".into(),
                        org: None
                    },
                    "数据清洗".to_string()
                ),
                (
                    MentionRef {
                        kind: MentionKind::Project,
                        id: "prj_1".into(),
                        org: None
                    },
                    "平台".to_string()
                ),
            ]
        );
    }

    #[test]
    fn resolved_mention_kind_label() {
        assert_eq!(
            ResolvedMention {
                kind: MentionKind::Agent,
                id: "x".into(),
                name: "张".into(),
                summary: None
            }
            .kind_label(),
            "Agent"
        );
        assert_eq!(
            ResolvedMention {
                kind: MentionKind::Task,
                id: "x".into(),
                name: "t".into(),
                summary: None
            }
            .kind_label(),
            "任务"
        );
    }
}
