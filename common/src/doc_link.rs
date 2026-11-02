//! 文档互引链接统一分类器（前后端复用，wasm 兼容，纯字符串解析）
//!
//! 唯一合法写法（AGENTS §2.1.2）：
//!   源码:   "相对仓库根路径#L起始-L结束"   例 "src/pkg/logging.rs#L15-L42"
//!   文档:   "相对仓库根路径.md"             例 "docs/design/logging_design.md"
//!   外链:   "http(s)://..."
//! legacy 兼容（存量，迁移后归零）：
//!   "path:15-42" / "path:L15-L42" / "file:///abs/path..." / "file://rel/path"

use std::borrow::Cow;

/// 行号范围（闭区间）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// 起始行（含）
    pub start: u32,
    /// 结束行（含）
    pub end: u32,
}

impl LineRange {
    /// 单行范围（start == end）
    pub fn single(line: u32) -> Self {
        Self {
            start: line,
            end: line,
        }
    }

    /// 输出 "#L15-L42" 或 "#L8" fragment
    pub fn to_fragment(self) -> String {
        if self.start == self.end {
            format!("#L{}", self.start)
        } else {
            format!("#L{}-L{}", self.start, self.end)
        }
    }
}

/// 链接分类结果（八类目标）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocLinkTarget {
    /// 仓库内源码文件（非 .md），可带行号范围
    SourceFile {
        /// 相对仓库根路径
        relative_path: String,
        /// 行号范围（可选）
        lines: Option<LineRange>,
    },
    /// 设计文档（docs/design/*.md）
    DesignDoc {
        /// 相对仓库根完整路径
        path: String,
    },
    /// 落地文档（docs/plan/*.md）
    PlanDoc {
        /// 相对仓库根完整路径
        path: String,
    },
    /// Wiki 百科长文（slug = 路径去掉 docs/wiki/zh/content/ 前缀与 .md 后缀）
    WikiArticle {
        /// 模块内相对 slug（可含子目录斜杠）
        slug: String,
    },
    /// RAG 原子知识卡（slug = 路径去掉 docs/wiki/knowledge/zh/ 前缀与 .md 后缀）
    RagCard {
        /// 模块内相对 slug（可含子目录斜杠）
        slug: String,
    },
    /// 其他文档（docs/ 下非四类标准位置的 .md，如 archive）
    OtherDoc {
        /// 相对仓库根路径
        relative_path: String,
    },
    /// 外部链接（http / mailto / ftp / 页内锚 #）
    External(String),
    /// 无法识别的非法链接（空串等）
    Invalid,
}

/// 前端路由信息（V2 内部路由升级时用；V1 前端走 GitHub 外链不经此映射）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendRouteInfo {
    /// 无路由（Invalid 输入）
    None,
    /// 纯外链（原样透传）
    External(String),
    /// 仓库内源码文件（V1 走 GitHub 外链）
    ExternalSource {
        /// 相对仓库根路径
        path: String,
        /// 行号范围（可选）
        lines: Option<LineRange>,
    },
    /// 文档中心设计文档页
    DocsDesign {
        /// 相对仓库根完整路径
        path: String,
    },
    /// 文档中心落地文档页
    DocsPlan {
        /// 相对仓库根完整路径
        path: String,
    },
    /// 文档中心 Wiki 长文页
    DocsWikiArticle {
        /// 模块内相对 slug
        slug: String,
    },
    /// 文档中心 RAG 卡页
    DocsRagCard {
        /// 模块内相对 slug
        slug: String,
    },
    /// 文档中心其他文档页
    DocsOther {
        /// 相对仓库根路径
        path: String,
    },
}

/// 文档互引链接统一分类器（纯静态方法，无状态）
pub struct DocLinkClassifier;

impl DocLinkClassifier {
    /// 分类一个 href 链接目标
    pub fn classify(href: &str) -> DocLinkTarget {
        let trimmed = href.trim();
        if trimmed.is_empty() {
            return DocLinkTarget::Invalid;
        }
        if trimmed.starts_with("http")
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("ftp:")
            || trimmed.starts_with('#')
        {
            return DocLinkTarget::External(trimmed.to_string());
        }
        let (path_part, lines) = Self::split_line_suffix(trimmed);
        let clean = Self::strip_legacy_prefix(&path_part);
        if clean.is_empty() {
            return DocLinkTarget::Invalid;
        }
        let lower = clean.to_ascii_lowercase();
        if !lower.ends_with(".md") {
            return DocLinkTarget::SourceFile {
                relative_path: clean.into_owned(),
                lines,
            };
        }
        if lower.starts_with("docs/design/") {
            DocLinkTarget::DesignDoc {
                path: clean.into_owned(),
            }
        } else if lower.starts_with("docs/plan/") {
            DocLinkTarget::PlanDoc {
                path: clean.into_owned(),
            }
        } else if lower.starts_with("docs/wiki/zh/content/") {
            let slug = clean["docs/wiki/zh/content/".len()..]
                .trim_end_matches(".md")
                .to_string();
            DocLinkTarget::WikiArticle { slug }
        } else if lower.starts_with("docs/wiki/knowledge/zh/") {
            let slug = clean["docs/wiki/knowledge/zh/".len()..]
                .trim_end_matches(".md")
                .to_string();
            DocLinkTarget::RagCard { slug }
        } else {
            DocLinkTarget::OtherDoc {
                relative_path: clean.into_owned(),
            }
        }
    }

    /// GitHub Web 输出：`<blob_base>/<path>#Lx-Ly`（fragment 原生兼容）
    pub fn to_github_url(target: &DocLinkTarget, blob_base: &str) -> String {
        match target {
            DocLinkTarget::External(u) => u.clone(),
            DocLinkTarget::Invalid => String::new(),
            _ => {
                let path = Self::to_relative_repo_path(target);
                let frag = match target {
                    DocLinkTarget::SourceFile { lines: Some(l), .. } => l.to_fragment(),
                    _ => String::new(),
                };
                format!("{blob_base}/{path}{frag}")
            }
        }
    }

    /// 前端路由信息（V2 内部路由升级时用；V1 前端走 GitHub 外链不经此函数）
    pub fn to_frontend_route_info(target: &DocLinkTarget) -> FrontendRouteInfo {
        match target {
            DocLinkTarget::External(u) => FrontendRouteInfo::External(u.clone()),
            DocLinkTarget::Invalid => FrontendRouteInfo::None,
            DocLinkTarget::SourceFile {
                relative_path,
                lines,
            } => FrontendRouteInfo::ExternalSource {
                path: relative_path.clone(),
                lines: *lines,
            },
            DocLinkTarget::DesignDoc { path } => {
                FrontendRouteInfo::DocsDesign { path: path.clone() }
            }
            DocLinkTarget::PlanDoc { path } => FrontendRouteInfo::DocsPlan { path: path.clone() },
            DocLinkTarget::WikiArticle { slug } => {
                FrontendRouteInfo::DocsWikiArticle { slug: slug.clone() }
            }
            DocLinkTarget::RagCard { slug } => {
                FrontendRouteInfo::DocsRagCard { slug: slug.clone() }
            }
            DocLinkTarget::OtherDoc { relative_path } => FrontendRouteInfo::DocsOther {
                path: relative_path.clone(),
            },
        }
    }

    // ===== 内部 =====

    /// 先试 `#Lx-Ly` fragment（主格式），再试 `:x-y` / `:Lx-Ly`（legacy）
    fn split_line_suffix(s: &str) -> (String, Option<LineRange>) {
        // Windows 盘符路径如 C:\x 不会出现在本仓库，find('#') 安全
        if let Some(hash_idx) = s.find('#')
            && let Some(range) = Self::parse_lines(&s[hash_idx + 1..])
        {
            return (s[..hash_idx].to_string(), Some(range));
        }
        if let Some(colon_idx) = s.rfind(':')
            && let Some(range) = Self::parse_lines(&s[colon_idx + 1..])
        {
            return (s[..colon_idx].to_string(), Some(range));
        }
        (s.to_string(), None)
    }

    /// 解析 "L15-L42" / "15-42" / "L8" / "8"；start/end 为 0 视为无效返回 None
    fn parse_lines(s: &str) -> Option<LineRange> {
        let cleaned = s.trim_start_matches(['L', 'l']);
        let (a, b) = match cleaned.split_once('-') {
            Some((a, b)) => (a, Some(b.trim_start_matches(['L', 'l']))),
            None => (cleaned, None),
        };
        let start: u32 = a.parse().ok()?;
        let end: u32 = match b {
            Some(x) => x.parse().ok()?,
            None => start,
        };
        if start == 0 || end == 0 {
            return None;
        }
        Some(LineRange {
            start: start.min(end),
            end: start.max(end),
        })
    }

    /// 剥离 legacy 前缀：file:/// + /ai_orz/ 绝对前缀、file:// 伪协议
    fn strip_legacy_prefix(s: &str) -> Cow<'_, str> {
        let t = s.strip_prefix("file://").unwrap_or(s);
        let t = t.strip_prefix('/').unwrap_or(t); // file:/// 的第三个斜杠
        if let Some(pos) = t.find("/ai_orz/") {
            Cow::Owned(t[pos + "/ai_orz/".len()..].to_string())
        } else {
            Cow::Borrowed(t)
        }
    }

    /// 目标 → 相对仓库根路径（External/Invalid 返回空串）
    fn to_relative_repo_path(t: &DocLinkTarget) -> String {
        match t {
            DocLinkTarget::SourceFile { relative_path, .. }
            | DocLinkTarget::OtherDoc { relative_path } => relative_path.clone(),
            DocLinkTarget::DesignDoc { path } | DocLinkTarget::PlanDoc { path } => path.clone(),
            DocLinkTarget::WikiArticle { slug } => format!("docs/wiki/zh/content/{slug}.md"),
            DocLinkTarget::RagCard { slug } => format!("docs/wiki/knowledge/zh/{slug}.md"),
            _ => String::new(),
        }
    }
}

/// 把 `./x` / `../y` 相对链接按「所在文档的仓库路径」归一化为相对仓库根路径。
///
/// 站内链接约定是**相对仓库根**（AGENTS §2.1.2），但部分文档写成了相对自身位置的
/// `./x.md`、`../y.md`。GitHub 能按文件位置解析这类链接，**文档中心不行**——md 副本
/// 被换成 `docs/modules/{hash}.md`，再拿原路径拼 blob URL 会得到 `blob/main/./x.md`
/// 这种 GitHub 解析不了的地址。所以导入期统一归一化一次。
///
/// - 非相对路径（`http(s)://`、`#锚点`、`mailto:`、已是仓库根相对）原样返回
/// - `..` 越过仓库根（弹栈到空还要弹）时原样返回，交给调用方兜底
/// - fragment / 查询串（`#L1-L5`、`?plain=1`）原样保留
///
/// `from_repo_path` 是文档自身的仓库相对路径，如 `docs/modules/02-Agent运行时/C-知识/x.md`。
pub fn normalize_repo_relative(path: &str, from_repo_path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("http")
        || trimmed.starts_with('#')
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("ftp:")
        || !(trimmed.starts_with("./") || trimmed.starts_with("../"))
    {
        return path.to_string();
    }

    let (rel, suffix) = match trimmed.find(['#', '?']) {
        Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
        None => (trimmed, ""),
    };

    // 基准目录 = 文档自身所在目录（不含文件名）
    let base_dir = from_repo_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut segments: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for segment in rel.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return path.to_string();
                }
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return path.to_string();
    }
    format!("{}{}", segments.join("/"), suffix)
}

/// 导入期改写：把 Markdown 里 `./` / `../` 形式的链接归一化为「相对仓库根」。
///
/// 围栏代码块（``` / ~~~）与行内代码里的示例链接**一律不动**——文档里大量
/// `[示例](./x.md)` 之类的教学片段，改了就变成假文档。支持 `](<path>)` 尖括号包裹
/// （路径含 `()` 的目录必须这样写，否则 Markdown 在首个 `)` 处截断）。
///
/// 幂等：已归一化的链接不以 `./`/`../` 开头，二次调用无变化。
pub fn rewrite_markdown_relative_links(md: &str, doc_repo_path: &str) -> String {
    let mut out = String::with_capacity(md.len() + 256);
    // 当前围栏标记（Some("```") / Some("~~~")），None 表示不在围栏内
    let mut fence: Option<&str> = None;
    for line in md.split_inclusive('\n') {
        let opener = line.trim_start();
        let marker = if opener.starts_with("```") {
            Some("```")
        } else if opener.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(m) = marker {
            match fence {
                None => fence = Some(m),
                // 只有同种标记能闭合
                Some(cur) if cur == m => fence = None,
                Some(_) => {}
            }
            out.push_str(line);
            continue;
        }
        if fence.is_some() {
            out.push_str(line);
        } else {
            out.push_str(&rewrite_line_links(line, doc_repo_path));
        }
    }
    out
}

/// 单行改写：按行内代码（反引号）切段，只在文本段里替换链接目标
fn rewrite_line_links(line: &str, doc_repo_path: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut buf = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            if in_code {
                out.push_str(&buf);
            } else {
                out.push_str(&rewrite_link_targets(&buf, doc_repo_path));
            }
            buf.clear();
            in_code = !in_code;
            out.push('`');
        } else {
            buf.push(ch);
        }
    }
    if in_code {
        out.push_str(&buf);
    } else {
        out.push_str(&rewrite_link_targets(&buf, doc_repo_path));
    }
    out
}

/// 在一段纯文本里找出 `](target)` / `](<target>)` 并归一化 target
fn rewrite_link_targets(text: &str, doc_repo_path: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find("](") {
        let (head, tail) = rest.split_at(idx + 2);
        out.push_str(head);
        let wrapped = tail.starts_with('<');
        let (target, consumed) = if wrapped {
            match tail.find('>') {
                Some(end) => (&tail[1..end], end + 1),
                None => {
                    out.push_str(tail);
                    return out;
                }
            }
        } else {
            match tail.find(')') {
                Some(end) => (&tail[..end], end),
                None => {
                    out.push_str(tail);
                    return out;
                }
            }
        };
        let rewritten = normalize_repo_relative(target, doc_repo_path);
        if wrapped {
            out.push('<');
            out.push_str(&rewritten);
            out.push('>');
        } else {
            out.push_str(&rewritten);
        }
        rest = &tail[consumed..];
    }
    out.push_str(rest);
    out
}
#[cfg(test)]
#[path = "doc_link_tests.rs"]
mod tests;
