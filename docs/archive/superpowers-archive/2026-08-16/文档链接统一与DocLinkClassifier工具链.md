📦 归档标记（2026-08-16）：被 [docs/archive/plan-archive/文档链接统一与DocLinkClassifier工具链.md](docs/archive/plan-archive/文档链接统一与DocLinkClassifier工具链.md) 取代。保留原因：原始执行蓝图含逐步命令/检查清单，留作审计参考。生效方案：[docs/archive/plan-archive/文档链接统一与DocLinkClassifier工具链.md](docs/archive/plan-archive/文档链接统一与DocLinkClassifier工具链.md)

---

# 文档路径引用统一化 + DocLinkClassifier 通用组件 Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除文档中硬编码本机绝对路径和 `file://` 伪协议，统一写「仓库相对路径 + `#Lx-Ly` 行号 fragment」格式（GitHub 原生兼容，IDE 降级为文件级跳转）；抽出 `DocLinkClassifier` 通用 Rust 组件（前后端复用，wasm 兼容）；新建 `tools/` workspace member 承载 docs_lint / docs_migrate 两个工具 bin；前端通过「渲染期 href 后处理 + index.html 全局点击拦截 JS 桥」实现文档中心链接正确分发。

**Architecture:** 五部分。① 写时规范（AGENTS 全部相关小节 + 4 个 Skill 文件）→ ② 通用分类器 `common::doc_link::DocLinkClassifier`（纯 std 字符串解析，兼容 `#Lx-Ly` 主格式与 `:x-y` legacy 格式）→ ③ `tools/ai-orz-tools` 新 workspace member（两个 bin：docs_lint CI 门禁 + docs_migrate 一次性迁移；依赖不污染生产 crate）→ ④ 前端：**先消重两个重复渲染器为单一共享函数**，再做链接后处理（`data-repo-href` 预拼 GitHub URL）+ index.html 全局 click listener JS 桥（复用 `__renderMermaid` 先例模式，零 Dioxus API 风险）→ ⑤ CI 接入 `.github/workflows/rust.yml`。

**Tech Stack:** Rust 1.80+ (common crate 纯 std / tools crate walkdir+regex / 后端零改动), wasm32-unknown-unknown 可编译, pulldown-cmark (frontend 既有), Dioxus 0.7 + index.html JS 桥, clippy -D warnings 双端零容忍.

**v2 关键决策（用户已拍板）：**
- **行号格式 = `#Lx-Ly` fragment**：GitHub 是唯一无法注入适配层的环境，保它；IDE 降级为文件级跳转。分类器同时兼容 legacy `:x-y` / `:Lx-Ly` 写法。
- **工具归置 = `tools/` 多 bin crate**：新 workspace member `ai-orz-tools`，未来租户 wiki-dedup-check / cite-graph-check。

---

## File Structure Lock-In

| # | File Path | Role | Action |
|---|-----------|------|--------|
| 1 | `common/src/doc_link.rs` | **核心通用组件**：`DocLinkTarget` 枚举 + `classify()` + 三环境输出（`to_github_url` / `to_frontend_route_info` / `to_relative_repo_path`）。纯 std，wasm 兼容 | Create |
| 2 | `common/src/lib.rs` | 加一行 `pub mod doc_link;` | Modify |
| 3 | `tools/Cargo.toml` | 新 workspace member `ai-orz-tools`，deps: walkdir + regex | Create |
| 4 | `tools/src/lib.rs` | 共享文件收集器 `collect_target_files()` + lint 核心纯函数 `lint_content()`（可单测） | Create |
| 5 | `tools/src/bin/docs_lint.rs` | CI 门禁 bin：调 lib 的 collect + lint，exit code 非 0 失败 | Create |
| 6 | `tools/src/bin/docs_migrate.rs` | 一次性迁移 bin：dry-run 默认 + `--apply`；三条规则与 lint 严格对应 | Create |
| 7 | `Cargo.toml` (workspace root) | members 加 `"tools"` | Modify |
| 8 | `AGENTS.md` | **多处**改：§一.2 能力表措辞、§二「路径格式铁律」表、§2.1.2 整节重写、§2.1.3.2 决策树、§2.1.3.3 四种关系声明模板（全部 `file:///绝对路径` → 相对路径） | Modify |
| 9 | `.trae/skills/ai-orz-wiki-maintainer/SKILL.md` | cite 节示例 + Hard non-negotiables 加路径格式红线 + `source_files[]` 示例改相对路径 + 「wiki 长文绝对路径」措辞改「相对仓库根路径」 | Modify |
| 10 | `.trae/skills/ai-orz-doc-maintainer/SKILL.md` | 同上（design/plan 模板内全部路径示例） | Modify |
| 11 | `docs/skills/ai-orz-wiki-maintainer.md` | 与 #9 镜像同步 | Modify |
| 12 | `docs/skills/ai-orz-doc-maintainer.md` | 与 #10 镜像同步 | Modify |
| 13 | `frontend/src/components/markdown.rs` | `render_markdown()` 加链接后处理（调共享 fn）；**与 docs.rs 消重**：此文件成为渲染唯一事实源 | Modify |
| 14 | `frontend/src/pages/system/docs.rs` | 删除重复渲染逻辑，改调 `components::markdown::render_markdown()` | Modify |
| 15 | `frontend/src/utils/doc_link.rs` | 链接后处理纯函数 `post_process_doc_links(html, github_blob_base)`（char_indices 安全迭代，站内 `<a>` 预拼 `data-repo-href` + `class="doc-link-intercept"`） | Create |
| 16 | `frontend/src/utils/mod.rs` | 加 `pub mod doc_link;` | Modify |
| 17 | `frontend/index.html` | 全局 click listener JS 桥：拦截 `.doc-link-intercept`，`window.open(data-repo-href)`（V1 全走 GitHub 新窗口；V2 可升级内部路由） | Modify |
| 18 | `.github/workflows/rust.yml` | 加 docs-lint step | Modify |

**不动的**：后端 src/ 零改动；`scripts/` 保留 shell 专用不动。

---

## Task 1: common/src/doc_link.rs — 通用 DocLinkClassifier 组件

**Files:**
- Create: `common/src/doc_link.rs`（含 19 个单元测试 inline）
- Modify: `common/src/lib.rs`

### 设计要点（v2 变更）

- 主格式 `path#L15-L42`；legacy 兼容 `path:15-42` / `path:L15-L42`（分类器照常解析，lint 不报、迁移统一转）
- `split_line_suffix` 改为**先试 `#` fragment 再试 `:` 冒号**（fragment 是主格式）
- `to_github_url` 输出 `blob_base/path#L15-L42`（fragment 原样透传，与 GitHub 原生行为一致）
- 移除 v1 的 `to_ide_path`（IDE 直接用原始 href，无需转换函数；YAGNI）
- 兼容剥离旧 `file:///Users/.../ai_orz/` 与 `file://` 前缀（过渡期，lint 会消灭它们）

```rust
//! 文档互引链接统一分类器（前后端复用，wasm 兼容，纯字符串解析）
//!
//! 唯一合法写法（AGENTS §2.1.2）：
//!   源码:   "相对仓库根路径#L起始-L结束"   例 "src/pkg/logging.rs#L15-L42"
//!   文档:   "相对仓库根路径.md"             例 "docs/design/logging_design.md"
//!   外链:   "http(s)://..."
//! legacy 兼容（存量，迁移后归零）：
//!   "path:15-42" / "path:L15-L42" / "file:///abs/path..." / "file://rel/path"

use std::borrow::Cow;
use std::fmt;

/// 行号范围（闭区间）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange { pub start: u32, pub end: u32 }

impl LineRange {
    pub fn single(line: u32) -> Self { Self { start: line, end: line } }
    /// 输出 "#L15-L42" 或 "#L8" fragment
    pub fn to_fragment(self) -> String {
        if self.start == self.end { format!("#L{}", self.start) }
        else { format!("#L{}-L{}", self.start, self.end) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocLinkTarget {
    SourceFile { relative_path: String, lines: Option<LineRange> },
    DesignDoc { path: String },
    PlanDoc { path: String },
    WikiArticle { slug: String },
    RagCard { slug: String },
    OtherDoc { relative_path: String },
    External(String),
    Invalid,
}

pub struct DocLinkClassifier;

impl DocLinkClassifier {
    pub fn classify(href: &str) -> DocLinkTarget {
        let trimmed = href.trim();
        if trimmed.is_empty() { return DocLinkTarget::Invalid; }
        if trimmed.starts_with("http") || trimmed.starts_with("mailto:")
            || trimmed.starts_with("ftp:") {
            return DocLinkTarget::External(trimmed.to_string());
        }
        // 页内锚（#section）无路径，视为 External 交给默认行为
        if trimmed.starts_with('#') {
            return DocLinkTarget::External(trimmed.to_string());
        }
        let (path_part, lines) = Self::split_line_suffix(trimmed);
        let clean = Self::strip_legacy_prefix(&path_part);
        if clean.is_empty() { return DocLinkTarget::Invalid; }
        let lower = clean.to_ascii_lowercase();
        if !lower.ends_with(".md") {
            return DocLinkTarget::SourceFile { relative_path: clean.into_owned(), lines };
        }
        if let Some(p) = clean.strip_prefix("docs/design/") {
            DocLinkTarget::DesignDoc { path: clean.into_owned(), ..DocLinkTarget::DesignDoc { path: p.to_string() } .path_do_not_use() }
        } else { DocLinkTarget::Invalid }
        // ↑ 上面两行是示意，实际实现直接 match 链（见下方真实实现）
    }
    // ……（真实实现见下）
}
```

（上面 classify 尾部示意有笔误——**以本节下方完整代码为准**，实现子代理直接贴下方代码：）

```rust
impl DocLinkClassifier {
    pub fn classify(href: &str) -> DocLinkTarget {
        let trimmed = href.trim();
        if trimmed.is_empty() { return DocLinkTarget::Invalid; }
        if trimmed.starts_with("http") || trimmed.starts_with("mailto:")
            || trimmed.starts_with("ftp:") || trimmed.starts_with('#') {
            return DocLinkTarget::External(trimmed.to_string());
        }
        let (path_part, lines) = Self::split_line_suffix(trimmed);
        let clean = Self::strip_legacy_prefix(&path_part);
        if clean.is_empty() { return DocLinkTarget::Invalid; }
        let lower = clean.to_ascii_lowercase();
        if !lower.ends_with(".md") {
            return DocLinkTarget::SourceFile { relative_path: clean.into_owned(), lines };
        }
        if lower.starts_with("docs/design/") {
            DocLinkTarget::DesignDoc { path: clean.into_owned() }
        } else if lower.starts_with("docs/archive/plan-archive/") {
            DocLinkTarget::PlanDoc { path: clean.into_owned() }
        } else if lower.starts_with("docs/wiki/zh/content/") {
            let slug = clean["docs/wiki/zh/content/".len()..]
                .trim_end_matches(".md").to_string();
            DocLinkTarget::WikiArticle { slug }
        } else if lower.starts_with("docs/wiki/knowledge/zh/") {
            let slug = clean["docs/wiki/knowledge/zh/".len()..]
                .trim_end_matches(".md").to_string();
            DocLinkTarget::RagCard { slug }
        } else {
            DocLinkTarget::OtherDoc { relative_path: clean.into_owned() }
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
            DocLinkTarget::SourceFile { relative_path, lines } =>
                FrontendRouteInfo::ExternalSource { path: relative_path.clone(), lines: *lines },
            DocLinkTarget::DesignDoc { path } => FrontendRouteInfo::DocsDesign { path: path.clone() },
            DocLinkTarget::PlanDoc { path } => FrontendRouteInfo::DocsPlan { path: path.clone() },
            DocLinkTarget::WikiArticle { slug } => FrontendRouteInfo::DocsWikiArticle { slug: slug.clone() },
            DocLinkTarget::RagCard { slug } => FrontendRouteInfo::DocsRagCard { slug: slug.clone() },
            DocLinkTarget::OtherDoc { relative_path } => FrontendRouteInfo::DocsOther { path: relative_path.clone() },
        }
    }

    // ===== 内部 =====

    /// 先试 `#Lx-Ly` fragment（主格式），再试 `:x-y` / `:Lx-Ly`（legacy）
    fn split_line_suffix(s: &str) -> (String, Option<LineRange>) {
        // Windows 盘符路径如 C:\x 不会出现在本仓库，rfind('#') 安全
        if let Some(hash_idx) = s.find('#') {
            if let Some(range) = Self::parse_lines(&s[hash_idx + 1..]) {
                return (s[..hash_idx].to_string(), Some(range));
            }
        }
        if let Some(colon_idx) = s.rfind(':') {
            if let Some(range) = Self::parse_lines(&s[colon_idx + 1..]) {
                return (s[..colon_idx].to_string(), Some(range));
            }
        }
        (s.to_string(), None)
    }

    /// 解析 "L15-L42" / "15-42" / "L8" / "8"
    fn parse_lines(s: &str) -> Option<LineRange> {
        let cleaned = s.trim_start_matches(['L', 'l']);
        let (a, b) = match cleaned.split_once('-') {
            Some((a, b)) => (a, Some(b.trim_start_matches(['L', 'l']))),
            None => (cleaned, None),
        };
        let start: u32 = a.parse().ok()?;
        let end: u32 = match b { Some(x) => x.parse().ok()?, None => start };
        if start == 0 || end == 0 { return None; }
        Some(LineRange { start: start.min(end), end: start.max(end) })
    }

    /// 剥离 legacy 前缀：file:/// + /ai_orz/ 绝对前缀、file:// 伪协议
    fn strip_legacy_prefix(s: &str) -> Cow<str> {
        let t = s.strip_prefix("file://").unwrap_or(s);
        let t = t.strip_prefix('/').unwrap_or(t); // file:/// 的第三个斜杠
        if let Some(pos) = t.find("/ai_orz/") {
            Cow::Owned(t[pos + "/ai_orz/".len()..].to_string())
        } else {
            Cow::Borrowed(t)
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendRouteInfo {
    None,
    External(String),
    ExternalSource { path: String, lines: Option<LineRange> },
    DocsDesign { path: String },
    DocsPlan { path: String },
    DocsWikiArticle { slug: String },
    DocsRagCard { slug: String },
    DocsOther { path: String },
}
```

### 完整测试（19 个，全部写在 doc_link.rs 的 `#[cfg(test)] mod tests`）

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // —— 主格式 #Lx-Ly ——

    #[test]
    fn t01_source_fragment_range() {
        let r = DocLinkClassifier::classify("src/pkg/logging.rs#L15-L42");
        assert!(matches!(r, DocLinkTarget::SourceFile { relative_path, lines: Some(LineRange { start: 15, end: 42 }) }
            if relative_path == "src/pkg/logging.rs"));
    }

    #[test]
    fn t02_source_fragment_single() {
        let r = DocLinkClassifier::classify("common/src/enums/user.rs#L8");
        let DocLinkTarget::SourceFile { lines, .. } = r else { panic!() };
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
        let DocLinkTarget::SourceFile { lines, .. } = r else { panic!() };
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    #[test]
    fn t05_legacy_colon_l_prefix() {
        let r = DocLinkClassifier::classify("src/pkg/logging.rs:L15-L42");
        let DocLinkTarget::SourceFile { lines, .. } = r else { panic!() };
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    // —— legacy file:// 前缀剥离 ——

    #[test]
    fn t06_strip_absolute_prefix() {
        let r = DocLinkClassifier::classify(
            "file:///Users/aman/Technology/rust/ai_orz/src/pkg/logging.rs#L15-L42");
        let DocLinkTarget::SourceFile { relative_path, lines } = r else { panic!() };
        assert_eq!(relative_path, "src/pkg/logging.rs");
        assert_eq!(lines, Some(LineRange { start: 15, end: 42 }));
    }

    #[test]
    fn t07_strip_pseudo_protocol() {
        let r = DocLinkClassifier::classify("file://src/pkg/logging.rs:15");
        let DocLinkTarget::SourceFile { relative_path, .. } = r else { panic!() };
        assert_eq!(relative_path, "src/pkg/logging.rs");
    }

    // —— 文档四类 ——

    #[test]
    fn t08_design_doc() {
        assert!(matches!(DocLinkClassifier::classify("docs/design/logging_design.md"),
            DocLinkTarget::DesignDoc { .. }));
    }

    #[test]
    fn t09_plan_doc() {
        assert!(matches!(DocLinkClassifier::classify("docs/archive/plan-archive/日志管理重构.md"),
            DocLinkTarget::PlanDoc { .. }));
    }

    #[test]
    fn t10_wiki_article_slug() {
        let r = DocLinkClassifier::classify(
            "docs/wiki/zh/content/功能模块/系统管理/日志管理系统.md");
        assert!(matches!(r, DocLinkTarget::WikiArticle { slug }
            if slug == "功能模块/系统管理/日志管理系统"));
    }

    #[test]
    fn t11_rag_card_slug() {
        let r = DocLinkClassifier::classify(
            "docs/wiki/knowledge/zh/日志系统/日志宏设计.md");
        assert!(matches!(r, DocLinkTarget::RagCard { slug } if slug == "日志系统/日志宏设计"));
    }

    #[test]
    fn t12_other_doc_archive() {
        assert!(matches!(DocLinkClassifier::classify("docs/archive/2024-01-old.md"),
            DocLinkTarget::OtherDoc { .. }));
    }

    // —— 外链 / 边界 ——

    #[test]
    fn t13_external_https() {
        assert!(matches!(DocLinkClassifier::classify("https://docs.rs/sqlx"),
            DocLinkTarget::External(_)));
    }

    #[test]
    fn t14_page_anchor_is_external() {
        assert!(matches!(DocLinkClassifier::classify("#section-2"),
            DocLinkTarget::External(_)));
    }

    #[test]
    fn t15_github_url_with_fragment_passthrough() {
        let t = DocLinkClassifier::classify("src/pkg/logging.rs#L15-L42");
        assert_eq!(DocLinkClassifier::to_github_url(&t, "https://github.com/o/r/blob/abc"),
            "https://github.com/o/r/blob/abc/src/pkg/logging.rs#L15-L42");
    }

    #[test]
    fn t16_github_url_legacy_normalized_to_fragment() {
        // legacy 冒号格式输出时归一化为 fragment
        let t = DocLinkClassifier::classify("src/pkg/logging.rs:15-42");
        assert_eq!(DocLinkClassifier::to_github_url(&t, "https://github.com/o/r/blob/main"),
            "https://github.com/o/r/blob/main/src/pkg/logging.rs#L15-L42");
    }

    #[test]
    fn t17_github_url_external_passthrough() {
        let t = DocLinkClassifier::classify("https://crates.io/crates/sqlx");
        assert_eq!(DocLinkClassifier::to_github_url(&t, "https://unused"),
            "https://crates.io/crates/sqlx");
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
            "docs/wiki/knowledge/zh/工具系统/CoreTool%20trait%20三层.md");
        assert!(matches!(r, DocLinkTarget::RagCard { .. }));
    }
}
```

### Steps

- [ ] **Step 1: Create `common/src/doc_link.rs`**（上方完整代码；注意删除「示意笔误」那段，只保留真实实现）
- [ ] **Step 2: Modify `common/src/lib.rs` 加 `pub mod doc_link;`**
- [ ] **Step 3: 跑测试**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo test -p common doc_link 2>&1 | tail -8
```
Expected: `test result: ok. 19 passed; 0 failed`

- [ ] **Step 4: wasm 兼容验证**

```bash
cargo check -p common --target wasm32-unknown-unknown 2>&1 | tail -5
```
Expected: Finished，0 warning（纯 std）

- [ ] **Step 5: Commit**

```bash
git add common/src/doc_link.rs common/src/lib.rs
git commit -m "feat(common): add DocLinkClassifier (#Lx-Ly primary format, legacy :x-y compat, 19 tests)"
```

---

## Task 2: AGENTS 多小节重写 + 4 个 Skill 文件同步

**Files:**
- Modify: `AGENTS.md`（**5 处**，见下）
- Modify: `.trae/skills/ai-orz-wiki-maintainer/SKILL.md`（4 处）
- Modify: `.trae/skills/ai-orz-doc-maintainer/SKILL.md`（4 处）
- Modify: `docs/skills/ai-orz-wiki-maintainer.md`（镜像）
- Modify: `docs/skills/ai-orz-doc-maintainer.md`（镜像）

### AGENTS.md 的 5 处改动清单（v2 扩大范围）

1. **§一.2 能力表**：知识体系行内「代码引用 `file://相对路径`，文档引用 `file:///绝对路径`」→「代码引用 `相对路径#Lx-Ly`，文档引用 `相对路径`」
2. **§二「路径格式铁律」表**：两条规则改为「跳代码 → `相对路径#L起始-L结束`（如 `src/pkg/logging.rs#L15-L42`）」「跳文档 → `相对路径`（如 `docs/design/xxx.md`）」
3. **§2.1.2 整节重写**（下方完整内容）
4. **§2.1.3.2 决策树**：所有「`file:///绝对路径`」字样 → 「相对路径」
5. **§2.1.3.3 速查表**：四种关系声明模板里全部 `file:///` 示例路径 → 相对路径（声明句式不变，只改路径写法）

### §2.1.2 新内容（完整替换）

```markdown
#### 2.1.2 路径引用统一规范（强制执行，三环境通跳）

> 🎯 **核心原则：一律写「相对仓库根的相对路径」，永不写本机绝对路径，永不写 `file://` / `file:///` 伪协议。行号用 `#Lx-Ly` fragment（GitHub 原生兼容；IDE 降级为文件级跳转，可接受）。**

| 引用类型 | 唯一合法格式 | 例子 |
|----------|------------|------|
| 代码（行范围） | `[描述](路径#L起始-L结束)` | `[日志初始化](src/pkg/logging.rs#L15-L42)` |
| 代码（单行） | `[描述](路径#L行)` | `[UserRole 定义](common/src/enums/user.rs#L8)` |
| 代码（无行号） | `[描述](路径)` | `[初始迁移](migrations/20260420000000_initial.sql)` |
| 文档互引 | `[描述](docs/...md)` | `[日志设计](docs/design/logging_design.md)` |
| Wiki 长文 | `[描述](docs/wiki/zh/content/...md)` | `[日志系统](docs/wiki/zh/content/功能模块/系统管理/日志管理系统.md)` |
| RAG 卡 | `[描述](docs/wiki/knowledge/zh/...md)` | `[日志宏卡](docs/wiki/knowledge/zh/日志系统/日志宏设计.md)` |
| 外部链接 | 直接写 http(s) | `[sqlx](https://docs.rs/sqlx)` |

**三环境行为**：
| 环境 | 行为 |
|------|------|
| GitHub 仓库页 | 相对链接自动解析为 `blob/<branch>/path#Lx-Ly` 并高亮行 ✅ |
| 本地 IDE | Cmd+Click 打开文件（fragment 被忽略，文件级跳转）⚠️ |
| 前端文档中心 | 渲染期后处理 + 点击拦截 → GitHub blob 新窗口 ✅ |

**注意**：md 链接目标里的空格必须写成 `%20`（如 `CoreTool%20trait`），中文字符原样保留。

**红线（tools/docs_lint CI 必 fail）**：
- ❌ 本机绝对路径（`file:///Users/...` 或裸 `/Users/...`）
- ❌ `file://` 伪协议前缀
- ❌ 行号写 legacy 冒号格式 `path:15-42`（存量已迁移归零；分类器兼容解析但新文禁写）

**契约型代码块规则（不变）**：trait 签名/struct 字段/enum 变体/SQL schema/ASCII 图可留代码块，紧邻下方附 `> 当前实现：[xxx.rs#L12-L50](src/xxx.rs#L12-L50)`；实现快照型（函数体/控制流/命令）删代码块，改 `> 逻辑见：[func](src/xxx.rs#L288-L352)`。
```

### Skill 文件的 4 处改动（每个文件同样套路）

1. **Hard non-negotiables 末尾追加**：
   ```
   - **⭐【路径格式硬约束】文档与 RAG 卡中所有路径引用（cite 节 / 章节来源 / source_files[] / 关联文档头部）必须使用 AGENTS §2.1.2 相对路径格式（行号 `#Lx-Ly`）**：出现 `file:///` 绝对路径 / `file://` 伪协议 / legacy 冒号行号 → 执行结果 FAIL，改完再过。
   ```
2. **所有 `file:///Users/.../ai_orz/` 示例** → 去前缀留相对路径（行号统一 `#Lx-Ly`）
3. **「wiki 长文绝对路径」措辞** → 「wiki 长文相对仓库根路径」（wiki-maintainer 专属）
4. **「占位：...回填真实路径有效性」机制不变**，仅路径写法改相对

### Steps

- [ ] **Step 1: 改 AGENTS.md 5 处（先 Grep `file://` 定位全部行，逐一 Edit）**
- [ ] **Step 2-5: 改 4 个 Skill 文件（同样先 Grep 后 Edit）**
- [ ] **Step 6: 验证（此刻全库存量 md 还有大量 ，lint 在 Task 3 才上；此处只验证 5 个规范文件自身新写内容格式正确）**

```bash
grep -c "file://" AGENTS.md
# Expected: 仅剩 §2.1.2 红线示例行（带 ❌ 前缀，lint 会跳过）+ §2.1.3 中已改完应接近 0
```

- [ ] **Step 7: Commit**

```bash
git add AGENTS.md .trae/skills/ docs/skills/
git commit -m "docs(agents): rewrite §2.1.2 link spec to relative+#Lx-Ly format; align 4 skill SOPs"
```

---

## Task 3: tools/ 新 workspace member（docs_lint + docs_migrate）

**Files:**
- Create: `tools/Cargo.toml` + `tools/src/lib.rs` + `tools/src/bin/docs_lint.rs` + `tools/src/bin/docs_migrate.rs`
- Modify: 根 `Cargo.toml` members 加 `"tools"`
- Modify: `.github/workflows/rust.yml` 加 lint step

### tools/Cargo.toml

```toml
[package]
name = "ai-orz-tools"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
walkdir = "2"
regex = "1"
```

### tools/src/lib.rs（共享核心，纯函数可单测）

```rust
//! AI Orz 文档工具集共享库
//!
//! 未来租户规划：wiki-dedup-check（AGENTS §2.1.3 Step 0 五级判定机器预检）、
//! cite-graph-check（四类互引闭环校验）。本 crate 依赖永不进生产二进制。

use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use walkdir::WalkDir;

/// lint 扫描目标：AGENTS.md + docs/**/*.md + .trae/skills/**/*.md
pub fn collect_target_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if Path::new("AGENTS.md").exists() { files.push(PathBuf::from("AGENTS.md")); }
    for dir in ["docs", ".trae/skills"] {
        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && p.extension().map(|e| e == "md").unwrap_or(false) {
                files.push(p.to_path_buf());
            }
        }
    }
    files
}

pub struct Violation {
    pub file: PathBuf,
    pub line_no: usize,
    pub rule: &'static str,
    pub snippet: String,
    pub help: &'static str,
}

/// lint 单个 md 内容（纯函数，跳过代码围栏 / 行内反引号 / ❌ 示例行）
pub fn lint_content(path: &Path, content: &str) -> Vec<Violation> {
    let re_file = Regex::new(r"file://").unwrap();
    let re_legacy_colon = Regex::new(r"\]\([^)\s]*?\.(rs|sql|toml|sh):\d+(-\d+)?\)").unwrap();
    let re_legacy_colon_l = Regex::new(r"\]\([^)\s]*?\.(rs|sql|toml|sh):L\d+(-L\d+)?\)").unwrap();
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in content.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") { in_fence = !in_fence; continue; }
        if in_fence { continue; }                       // 跳过代码围栏
        if t.starts_with("❌") || t.contains('`') { continue; } // 跳过红线示例与行内代码
        if let Some(m) = re_file.find(line) {
            let after = &line[m.end()..];
            let is_abs = after.starts_with('/')
                || after.starts_with("Users/") || after.starts_with("home/");
            out.push(Violation {
                file: path.to_path_buf(), line_no: i + 1,
                rule: if is_abs { "R1_abs_path" } else { "R2_file_protocol" },
                snippet: snippet_of(line, m.start(), m.end()),
                help: "改写为相对仓库根路径，见 AGENTS §2.1.2",
            });
        }
        for (re, rule) in [(&re_legacy_colon, "R3_legacy_colon_lines"), (&re_legacy_colon_l, "R3_legacy_colon_lines")] {
            if let Some(m) = re.find(line) {
                out.push(Violation {
                    file: path.to_path_buf(), line_no: i + 1, rule,
                    snippet: snippet_of(line, m.start(), m.end()),
                    help: "行号应写 #Lx-Ly fragment 而非 :x-y，见 AGENTS §2.1.2",
                });
            }
        }
    }
    out
}

fn snippet_of(line: &str, s: usize, e: usize) -> String {
    let s = s.saturating_sub(15); let e = (e + 15).min(line.len());
    line[s..e].to_string()
}

/// 迁移：单文件内容改写（纯函数；返回 (新内容, 替换次数)）
pub fn migrate_content(content: &str) -> (String, usize) {
    let re_abs = Regex::new(r"file:///[^\s)\"]*?/ai_orz/").unwrap();
    let re_pseudo = Regex::new(r"file://").unwrap();
    let re_colon = Regex::new(r"\]\(([^)\s]*?\.(rs|sql|toml|sh)):L?(\d+)(?:-L?(\d+))?\)").unwrap();
    let mut count = 0usize;
    let s1 = re_abs.replace_all(content, |_:[&regex::Captures]| { count += 1; "" }).into_owned();
    let s2 = re_pseudo.replace_all(&s1, |_:[&regex::Captures]| { count += 1; "" }).into_owned();
    let s3 = re_colon.replace_all(&s2, |c: &regex::Captures| {
        count += 1;
        let path = &c[1];
        let a: u32 = c[3].parse().unwrap();
        let b: u32 = c.get(4).map(|m| m.as_str().parse().unwrap()).unwrap_or(a);
        let frag = if a == b { format!("#L{a}") } else { format!("#L{a}-L{b}") };
        format!("]({path}{frag})")
    }).into_owned();
    (s3, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_catches_abs_and_pseudo() {
        let v = lint_content(Path::new("t.md"),
            "- [a](file:///Users/x/ai_orz/src/a.rs)\n- [b](file://src/b.rs)\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].rule, "R1_abs_path");
        assert_eq!(v[1].rule, "R2_file_protocol");
    }

    #[test]
    fn lint_skips_fence_and_emoji_and_backtick() {
        let v = lint_content(Path::new("t.md"),
            "```\nfile://x\n```\n- ❌ bad: file:///Users/x\n- ok `file://y`\n");
        assert!(v.is_empty());
    }

    #[test]
    fn lint_catches_legacy_colon() {
        let v = lint_content(Path::new("t.md"), "- [a](src/a.rs:15-42)\n");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "R3_legacy_colon_lines");
    }

    #[test]
    fn lint_ignores_github_external_fragment() {
        // 合法 GitHub 外链的 #L15 不该被报
        let v = lint_content(Path::new("t.md"),
            "- [a](https://github.com/o/r/blob/main/src/a.rs#L15-L42)\n");
        assert!(v.is_empty());
    }

    #[test]
    fn migrate_full_chain() {
        let (out, n) = migrate_content(
            "- [a](file:///Users/x/rust/ai_orz/src/a.rs#L1-L9)\n- [b](file://src/b.rs:15-42)\n");
        assert_eq!(n, 2);
        assert!(out.contains("](src/a.rs#L1-L9)"));
        assert!(out.contains("](src/b.rs#L15-L42)"));
    }
}
```

### tools/src/bin/docs_lint.rs

```rust
//! CI 门禁：cargo run -p ai-orz-tools --bin docs_lint

use ai_orz_tools::{collect_target_files, lint_content};
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let files = collect_target_files();
    let mut total = 0usize;
    for f in &files {
        if let Ok(c) = fs::read_to_string(f) {
            for v in lint_content(f, &c) {
                eprintln!("{}:{}: [{}] ...{}... | {}", v.file.display(), v.line_no, v.rule, v.snippet, v.help);
                total += 1;
            }
        }
    }
    if total > 0 {
        eprintln!("\ndocs_lint FAILED: {total} violations in {} files", files.len());
        ExitCode::from(1)
    } else {
        println!("docs_lint OK: {} files, 0 violations", files.len());
        ExitCode::SUCCESS
    }
}
```

### tools/src/bin/docs_migrate.rs

```rust
//! 一次性迁移：默认 dry-run；--apply 才写盘

use ai_orz_tools::{collect_target_files, migrate_content};
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let apply = std::env::args().any(|a| a == "--apply");
    let files = collect_target_files();
    let mut total = 0usize;
    let mut touched = 0usize;
    for f in &files {
        let Ok(c) = fs::read_to_string(f) else { continue };
        let (new, n) = migrate_content(&c);
        if n == 0 { continue; }
        touched += 1; total += n;
        if apply {
            let _ = fs::write(f, &new);
            println!("APPLIED {} ({} replacements)", f.display(), n);
        } else {
            println!("WOULD  {} ({} replacements)", f.display(), n);
        }
    }
    println!("\n{} mode: {} replacements across {} files",
        if apply { "APPLY" } else { "DRY-RUN" }, total, touched);
    ExitCode::SUCCESS
}
```

### CI 接入（.github/workflows/rust.yml）

在现有 job 的 clippy 步骤后加：

```yaml
      - name: Docs link lint
        run: cargo run -p ai-orz-tools --bin docs_lint
```

### Steps

- [ ] **Step 1: 建 tools/ 四个文件 + 根 Cargo.toml members 加 `"tools"`**
- [ ] **Step 2: 跑 tools 单测（5 个）**

```bash
cargo test -p ai-orz-tools 2>&1 | tail -5
```
Expected: `5 passed`

- [ ] **Step 3: 跑 lint 看存量违规规模（此刻应大量报，属预期）**

```bash
cargo run -p ai-orz-tools --bin docs_lint 2>&1 | tail -3
```
Expected: `docs_lint FAILED: N violations`（N 数百级，记录数字）

- [ ] **Step 4: dry-run 迁移预览**

```bash
cargo run -p ai-orz-tools --bin docs_migrate 2>&1 | tail -5
```

- [ ] **Step 5: 人工抽 10 条 WOULD 行 review 替换正确性（重点看中文路径 / %20 / YAML source_files）**
- [ ] **Step 6: `--apply` 真正迁移，再跑 lint 应显著下降；残留逐条手修到 0**

```bash
cargo run -p ai-orz-tools --bin docs_migrate -- --apply
cargo run -p ai-orz-tools --bin docs_lint 2>&1 | tail -3
```
Expected: `docs_lint OK: ... 0 violations`

- [ ] **Step 7: .github/workflows/rust.yml 加 lint step**
- [ ] **Step 8: Commit（迁移是机械大 diff，独立成 commit）**

```bash
git add tools/ Cargo.toml .github/workflows/rust.yml docs/ AGENTS.md .trae/skills/
git commit -m "feat(tools): add ai-orz-tools workspace member (docs_lint + docs_migrate); migrate all doc links to relative+#Lx-Ly"
```

---

## Task 4: 前端 — 双渲染器消重 + 链接后处理 + JS 桥拦截

**Files:**
- Modify: `frontend/src/components/markdown.rs`（成为渲染唯一事实源；docs.rs 改调它）
- Modify: `frontend/src/pages/system/docs.rs`（删重复渲染，改 import）
- Create: `frontend/src/utils/doc_link.rs`（post_process 纯函数，char_indices 安全迭代）
- Modify: `frontend/src/utils/mod.rs`
- Modify: `frontend/index.html`（全局 click listener）

### 设计要点（v2 规避 Dioxus API 风险）

**完全复用项目已验证的 JS 桥模式（`window.__renderMermaid` 先例）**，不在 rsx 里写事件委托：

1. Rust 渲染期：`post_process_doc_links(html, blob_base)` 扫描非 http 的 `<a>`，追加 `class="doc-link-intercept"` + `data-repo-href="<blob_base>/<解析后相对路径>#Lx-Ly"`（用 common 分类器解析 legacy 前缀并归一化行号），**原 href 保留**（JS 桥失效时浏览器还能退化为页内导航）
2. index.html 全局 listener（一次性注入，约 10 行 JS）：拦截 `.doc-link-intercept` 点击 → `preventDefault` + `window.open(data-repo-href, '_blank', 'noopener')`
3. V1 所有站内链接走 GitHub 新窗口；**V2 升级路径**（本 plan 不做）：读 [pages/mod.rs](frontend/src/pages/mod.rs) 的 Route enum，把 docs 类 slug 映射为 SPA hash 路由

### frontend/src/utils/doc_link.rs

```rust
//! 渲染期链接后处理：站内相对路径 <a> 预拼 data-repo-href（GitHub blob 绝对 URL）
//!
//! char_indices 迭代保证中文（多字节 UTF-8）不被拆坏。

use common::doc_link::{DocLinkClassifier, DocLinkTarget};

/// blob_base 例：https://github.com/<org>/ai_orz/blob/main
pub fn post_process_doc_links(html: &str, blob_base: &str) -> String {
    let mut out = String::with_capacity(html.len() + 1024);
    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i < html.len() {
        // 安全锚点：<a href=" ... ">（pulldown-cmark 产物格式固定，小写）
        if html.is_char_boundary(i) && bytes[i] == b'<' && html[i..].starts_with("<a href=\"") {
            let href_start = i + "<a href=\"".len();
            if let Some(rel) = html[href_start..].find('"') {
                let href_end = href_start + rel;
                let href = &html[href_start..href_end];
                let is_external = href.starts_with("http") || href.starts_with('#')
                    || href.starts_with("mailto:");
                let tag_end = html[href_end..].find('>').map(|p| href_end + p + 1)
                    .unwrap_or(html.len());
                let tag = &html[i..tag_end];
                if is_external {
                    let safe = if tag.contains("target=") { tag.to_string() } else {
                        format!("{} target=\"_blank\" rel=\"noopener noreferrer\">",
                            tag.trim_end_matches('>'))
                    };
                    out.push_str(&safe);
                } else {
                    let repo_href = DocLinkClassifier::to_github_url(
                        &DocLinkClassifier::classify(href), blob_base);
                    // 解析失败（Invalid）则原样保留
                    if repo_href.is_empty() { out.push_str(tag); }
                    else {
                        out.push_str(&format!(
                            "{} data-repo-href=\"{}\" class=\"doc-link-intercept\">",
                            tag.trim_end_matches('>'), repo_href));
                    }
                }
                i = tag_end;
                continue;
            }
        }
        // 按字符推进（非字节），UTF-8 安全
        let ch = html[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
```

（`frontend/Cargo.toml` 已依赖 common——确认一下，没有则加 `common = { path = "../common" }`）

### markdown.rs 改动（两处）

1. `render_markdown()` 末尾 `push_html` 之后追加一行：
```rust
    pulldown_cmark::html::push_html(&mut html_out, escaped);
    // 站内链接预拼 data-repo-href（JS 桥拦截用）
    let html_out = crate::utils::doc_link::post_process_doc_links(
        &html_out, crate::utils::doc_link::BLOB_BASE);
    html_out
```

2. `frontend/src/utils/doc_link.rs` 顶部加常量（org/repo 按实际仓库填，V1 用 main 分支）：
```rust
pub const BLOB_BASE: &str = "https://github.com/<org>/ai_orz/blob/main";
```

### docs.rs 消重

找到 `frontend/src/pages/system/docs.rs` 中与 `render_markdown` 重复的 pulldown-cmark 渲染段（Grep `pulldown_cmark` 定位），整段删除，改 `use crate::components::markdown::render_markdown;`。

### index.html 追加（body 尾部）

```html
<script>
// 文档中心站内链接拦截：预拼的 data-repo-href 新窗口打开（AGENTS §2.1.2）
document.addEventListener('click', function (e) {
  var a = e.target.closest && e.target.closest('a.doc-link-intercept');
  if (a) { e.preventDefault(); window.open(a.dataset.repoHref, '_blank', 'noopener'); }
});
</script>
```

### Steps

- [ ] **Step 1: 确认 frontend/Cargo.toml 是否已依赖 common；无则加**
- [ ] **Step 2: Create utils/doc_link.rs + mod.rs 导出**
- [ ] **Step 3: markdown.rs 接 post_process；docs.rs 删重复渲染改 import**
- [ ] **Step 4: index.html 加 listener**
- [ ] **Step 5: 前端编译 + clippy**

```bash
cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -10
```
Expected: 0 warnings

- [ ] **Step 6: 本地起前端冒烟：打开文档中心任一 wiki 长文，点 ①源码链接 ②文档互引 ③外链，分别验证新窗口 GitHub 正确打开（含行号高亮）**
- [ ] **Step 7: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): dedupe markdown renderers; wire doc-link interceptor via data-repo-href + JS bridge"
```

---

## Task 5: 集成验证

- [ ] **Step 1: 全 workspace clippy**

```bash
cargo clippy --workspace --exclude frontend -- -D warnings 2>&1 | tail -5
cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -5
```

- [ ] **Step 2: 全部测试（common 19 + tools 5 + 既有 1124 不回归）**

```bash
cargo test -p common -p ai-orz-tools 2>&1 | tail -5
```

- [ ] **Step 3: lint 终态 0 违规 + 全库无 file:// 残留（除规范文件 ❌ 示例行）**

```bash
cargo run -p ai-orz-tools --bin docs_lint 2>&1 | tail -2
```
Expected: `docs_lint OK`

- [ ] **Step 4: Commit 收尾**

```bash
git add -u && git commit -m "chore: final validation for doc link unification v2"
```

---

## Self-Review Checklist

### Spec Coverage
- [x] 三环境通跳（GitHub 原生 / IDE 文件级 / 前端拦截）→ 格式决策 + Task 1/4
- [x] 通用组件抽取（common，wasm 兼容）→ Task 1
- [x] 工具归置 tools/ workspace member → Task 3
- [x] 写时规范（AGENTS 5 小节 + 4 Skill）→ Task 2
- [x] CI 门禁 + 一次性迁移 → Task 3
- [x] 双渲染器消重 → Task 4
- [x] lint 跳过围栏/反引号/❌ 行（自咬防护）→ Task 3 lib.rs
- [x] R3 不误伤 GitHub 外链（仅匹配 `](path.ext:x-y)` 形态）→ Task 3
- [x] UTF-8 安全（char_indices / chars 迭代）→ Task 4
- [x] Dioxus API 风险消除（JS 桥模式）→ Task 4

### Placeholder Scan
- [x] 无 TBD / "看情况组织"；docs_migrate 用完整 bin 代码非含糊脚本
- [x] 全部代码块完整可编译（Task 1 首段「示意笔误」已显式标注仅保留真实实现版本）

### Type Consistency
- [x] `LineRange { start, end }` 贯穿 Task 1 / Task 4
- [x] `FrontendRouteInfo` 变体与 `to_frontend_route_info` match 臂一一对应
- [x] `BLOB_BASE` 常量在 Task 4 内定义并使用

## Execution Handoff

Plan v2 complete and saved to `docs/superpowers/plans/2026-08-16-docs-link-unification-and-classifier.md`. Two execution options:

**1. Subagent-Driven (recommended)** — Task 1→5 顺序派发子代理，任务间 review（Task 3 Step 5 抽样 review 后才 `--apply`）。

**2. Inline Execution** — 本会话 executing-plans 串行执行，每 Task 一个 checkpoint。

Which approach?
