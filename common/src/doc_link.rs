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

#[cfg(test)]
mod tests {
    use super::*;

    // —— 主格式 #Lx-Ly ——

    #[test]
    fn t01_source_fragment_range() {
        let r = DocLinkClassifier::classify("src/pkg/logging.rs#L15-L42");
        assert!(
            matches!(r, DocLinkTarget::SourceFile { relative_path, lines: Some(LineRange { start: 15, end: 42 }) }
            if relative_path == "src/pkg/logging.rs")
        );
    }

    #[test]
    fn t02_source_fragment_single() {
        let r = DocLinkClassifier::classify("common/src/enums/user.rs#L8");
        let DocLinkTarget::SourceFile { lines, .. } = r else {
            panic!()
        };
        assert_eq!(lines, Some(LineRange::single(8)));
    }

    #[test]
    fn t03_source_no_lines() {
        let r = DocLinkClassifier::classify("migrations/20260420000000_initial.sql");
        assert!(matches!(r, DocLinkTarget::SourceFile { lines: None, .. }));
    }

    // —— legacy 冒号格式（兼容解析，不推荐写） ——

    #[test]
    fn t04_legacy_colon_range() {
        let r = DocLinkClassifier::classify("src/pkg/logging.rs:15-42");
        let DocLinkTarget::SourceFile { lines, .. } = r else {
            panic!()
        };
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    #[test]
    fn t05_legacy_colon_l_prefix() {
        let r = DocLinkClassifier::classify("src/pkg/logging.rs:L15-L42");
        let DocLinkTarget::SourceFile { lines, .. } = r else {
            panic!()
        };
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    // —— legacy file:// 前缀剥离 ——

    #[test]
    fn t06_strip_absolute_prefix() {
        let r = DocLinkClassifier::classify(
            "file:///Users/aman/Technology/rust/ai_orz/src/pkg/logging.rs#L15-L42",
        );
        let DocLinkTarget::SourceFile {
            relative_path,
            lines,
        } = r
        else {
            panic!()
        };
        assert_eq!(relative_path, "src/pkg/logging.rs");
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    #[test]
    fn t07_strip_pseudo_protocol() {
        let r = DocLinkClassifier::classify("file://src/pkg/logging.rs:15");
        let DocLinkTarget::SourceFile { relative_path, .. } = r else {
            panic!()
        };
        assert_eq!(relative_path, "src/pkg/logging.rs");
    }

    // —— 文档四类 ——

    #[test]
    fn t08_design_doc() {
        assert!(matches!(
            DocLinkClassifier::classify("docs/design/logging_design.md"),
            DocLinkTarget::DesignDoc { .. }
        ));
    }

    #[test]
    fn t09_plan_doc() {
        assert!(matches!(
            DocLinkClassifier::classify("docs/plan/日志管理重构.md"),
            DocLinkTarget::PlanDoc { .. }
        ));
    }

    #[test]
    fn t10_wiki_article_slug() {
        let r =
            DocLinkClassifier::classify("docs/wiki/zh/content/功能模块/系统管理/日志管理系统.md");
        assert!(matches!(r, DocLinkTarget::WikiArticle { slug }
            if slug == "功能模块/系统管理/日志管理系统"));
    }

    #[test]
    fn t11_rag_card_slug() {
        let r = DocLinkClassifier::classify("docs/wiki/knowledge/zh/日志系统/日志宏设计.md");
        assert!(matches!(r, DocLinkTarget::RagCard { slug } if slug == "日志系统/日志宏设计"));
    }

    #[test]
    fn t12_other_doc_archive() {
        assert!(matches!(
            DocLinkClassifier::classify("docs/archive/2024-01-old.md"),
            DocLinkTarget::OtherDoc { .. }
        ));
    }

    // —— 外链 / 边界 ——

    #[test]
    fn t13_external_https() {
        assert!(matches!(
            DocLinkClassifier::classify("https://docs.rs/sqlx"),
            DocLinkTarget::External(_)
        ));
    }

    #[test]
    fn t14_page_anchor_is_external() {
        assert!(matches!(
            DocLinkClassifier::classify("#section-2"),
            DocLinkTarget::External(_)
        ));
    }

    #[test]
    fn t15_github_url_with_fragment_passthrough() {
        let t = DocLinkClassifier::classify("src/pkg/logging.rs#L15-L42");
        assert_eq!(
            DocLinkClassifier::to_github_url(&t, "https://github.com/o/r/blob/abc"),
            "https://github.com/o/r/blob/abc/src/pkg/logging.rs#L15-L42"
        );
    }

    #[test]
    fn t16_github_url_legacy_normalized_to_fragment() {
        // legacy 冒号格式输出时归一化为 fragment
        let t = DocLinkClassifier::classify("src/pkg/logging.rs:15-42");
        assert_eq!(
            DocLinkClassifier::to_github_url(&t, "https://github.com/o/r/blob/main"),
            "https://github.com/o/r/blob/main/src/pkg/logging.rs#L15-L42"
        );
    }

    #[test]
    fn t17_github_url_external_passthrough() {
        let t = DocLinkClassifier::classify("https://crates.io/crates/sqlx");
        assert_eq!(
            DocLinkClassifier::to_github_url(&t, "https://unused"),
            "https://crates.io/crates/sqlx"
        );
    }

    #[test]
    fn t18_empty_invalid() {
        assert_eq!(DocLinkClassifier::classify(""), DocLinkTarget::Invalid);
        assert_eq!(DocLinkClassifier::classify("  "), DocLinkTarget::Invalid);
    }

    #[test]
    fn t19_url_encoded_space_kept() {
        // %20 编码必须原样保留（md 链接目标里空格必须编码）
        let r = DocLinkClassifier::classify(
            "docs/wiki/knowledge/zh/工具系统/CoreTool%20trait%20三层.md",
        );
        assert!(matches!(r, DocLinkTarget::RagCard { .. }));
    }
}
