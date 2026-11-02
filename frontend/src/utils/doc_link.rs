//! 渲染期链接后处理：源文相对链接归一化 + 站内/远程统一分发
//!
//! 源文档按 GitHub 语义写**相对自身位置**的链接（`./x.md`、`../y.md`），GitHub 能
//! 按文件位置解析。文档中心的 md 副本是 `docs/{hash}.md`，原路径必然解析错，所以在
//! 渲染期统一处理一次（打包期只复制字节 + 记录 `repo_path`，不改内容）：
//!
//! - 外链（http / # / mailto）：新标签原样打开
//! - 仓库内 Markdown（已收录）：站内原地切换（`data-doc-path` + Dioxus 点击拦截）
//! - 仓库内 Markdown（未收录，如 archive）：退化 GitHub blob
//! - 源码文件（`.rs` / `.sql` 等）：GitHub blob + `#Lx-Ly`
//! - 架构图 `docs/diagrams/*.html`：站内新标签直开
//!
//! 站内链接保留 GitHub 地址作为 href 降级；拦截失效时仍能跳远程。
//!
//! 按 chars 迭代推进 + is_char_boundary 锚点检查，保证中文（多字节 UTF-8）不被拆坏。

use std::collections::HashMap;
use std::rc::Rc;

use common::doc_link::{DocLinkClassifier, DocLinkTarget, normalize_repo_relative};

/// GitHub blob 前缀（仓库内源码 / 未收录文档的远程地址）
// 与 git remote origin（git@github.com:runningcoders/ai_orz.git）对齐；换 remote 时同步改
pub const BLOB_BASE: &str = "https://github.com/runningcoders/ai_orz/blob/main";

/// 文档中心渲染上下文：当前文档的仓库路径 + 仓库路径 → 站内副本路径映射
///
/// `repo_to_page` 由 docs.rs 从 index.json 构建（build.rs 打包时写入 `repo_path`）。
#[derive(Clone)]
pub struct DocRenderContext {
    /// 当前文档自身的仓库相对路径（`./` `../` 归一化的基准）
    pub doc_repo_path: String,
    /// 仓库路径 → 站内副本路径（`modules/{hash}.md`）
    pub repo_to_page: Rc<HashMap<String, String>>,
}

// 映射表整表是构建期产物、实例内不变，故只比对指针，避免每次渲染深比较几百条路径
impl PartialEq for DocRenderContext {
    fn eq(&self, other: &Self) -> bool {
        self.doc_repo_path == other.doc_repo_path
            && Rc::ptr_eq(&self.repo_to_page, &other.repo_to_page)
    }
}

/// 对 pulldown-cmark 产物 HTML 做链接后处理
///
/// - 外链补 `target="_blank" rel="noopener noreferrer"`（已有 target 则不动）
/// - 站内链接按 [`DocRenderContext`] 归一化并分发；无上下文时保持旧的 GitHub 直跳
/// - `classify` 结果 Invalid 则原样保留
pub fn post_process_doc_links(
    html: &str,
    blob_base: &str,
    doc_ctx: Option<&DocRenderContext>,
) -> String {
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
                let is_external = href.starts_with("http")
                    || href.starts_with('#')
                    || href.starts_with("mailto:");
                let tag_end = html[href_end..]
                    .find('>')
                    .map(|p| href_end + p + 1)
                    .unwrap_or(html.len());
                let tag = &html[i..tag_end];
                if is_external {
                    let safe = if tag.contains("target=") {
                        tag.to_string()
                    } else {
                        format!(
                            "{} target=\"_blank\" rel=\"noopener noreferrer\">",
                            tag.trim_end_matches('>')
                        )
                    };
                    out.push_str(&safe);
                } else {
                    // 源文可能写的是相对自身位置的 `./` `../`：先按当前文档仓库路径归一化
                    let normalized = match doc_ctx {
                        Some(ctx) => normalize_repo_relative(href, &ctx.doc_repo_path),
                        None => href.to_string(),
                    };
                    let target = DocLinkClassifier::classify(&normalized);
                    // 架构图是自包含 HTML，构建期已原路径复制到 public/docs/diagrams/：
                    // 直接站内新标签打开渲染页，比跳 GitHub 看一堆 HTML 源码有用
                    // （classify 把非 .md 一律归为 SourceFile，两类都要看）
                    let diagram = match &target {
                        DocLinkTarget::SourceFile { relative_path, .. }
                        | DocLinkTarget::OtherDoc { relative_path } => relative_path
                            .strip_prefix("docs/diagrams/")
                            .filter(|rest| rest.ends_with(".html")),
                        _ => None,
                    };
                    if let Some(rest) = diagram {
                        let static_href = format!("/docs/diagrams/{rest}");
                        out.push_str(&tag.replacen(
                            &format!("href=\"{href}\""),
                            &format!(
                                "href=\"{static_href}\" target=\"_blank\" rel=\"noopener noreferrer\""
                            ),
                            1,
                        ));
                        i = tag_end;
                        continue;
                    }
                    // 已收录的 Markdown：站内原地切换；href 保留 GitHub 作降级
                    if let Some((page, repo_path)) = internal_doc_page(&target, doc_ctx) {
                        let fallback = DocLinkClassifier::to_github_url(&target, blob_base);
                        let tag_with_fallback = tag.replacen(
                            &format!("href=\"{href}\""),
                            &format!("href=\"{fallback}\""),
                            1,
                        );
                        out.push_str(tag_with_fallback.trim_end_matches('>'));
                        out.push_str(&format!(
                            " data-doc-path=\"{page}\" data-doc-repo=\"{repo_path}\" \
                             class=\"doc-link-internal\">"
                        ));
                        i = tag_end;
                        continue;
                    }
                    let repo_href = DocLinkClassifier::to_github_url(&target, blob_base);
                    // 解析失败（Invalid → to_github_url 返回空串）则原样保留
                    if repo_href.is_empty() {
                        out.push_str(tag);
                    } else {
                        out.push_str(&format!(
                            "{} data-repo-href=\"{}\" class=\"doc-link-intercept\">",
                            tag.trim_end_matches('>'),
                            repo_href
                        ));
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

/// 已收录的 Markdown → (站内副本路径, 原始仓库路径)
///
/// 未命中映射（如 docs/archive/ 未收录、源码文件）返回 None，交给调用方退化远程。
fn internal_doc_page(
    target: &DocLinkTarget,
    doc_ctx: Option<&DocRenderContext>,
) -> Option<(String, String)> {
    // 只有 Markdown 类目标才可能站内切换
    if !matches!(
        target,
        DocLinkTarget::DesignDoc { .. }
            | DocLinkTarget::PlanDoc { .. }
            | DocLinkTarget::WikiArticle { .. }
            | DocLinkTarget::RagCard { .. }
            | DocLinkTarget::OtherDoc { .. }
    ) {
        return None;
    }
    let ctx = doc_ctx?;
    let repo_path = DocLinkClassifier::to_repo_path(target)?;
    // 链接里的中文/空格通常被百分号编码，映射键是文件系统原始名
    let decoded = percent_decode(&repo_path);
    ctx.repo_to_page
        .get(&decoded)
        .map(|page| (page.clone(), decoded))
}

/// 百分号解码（仅处理 `%XX`；非法序列原样保留，供映射查找）
fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(doc_repo_path: &str, pages: &[(&str, &str)]) -> DocRenderContext {
        let map = pages
            .iter()
            .map(|(repo, page)| (repo.to_string(), page.to_string()))
            .collect();
        DocRenderContext {
            doc_repo_path: doc_repo_path.to_string(),
            repo_to_page: Rc::new(map),
        }
    }

    #[test]
    fn internal_relative_link_gets_repo_href() {
        let html = "<p>见 <a href=\"src/pkg/logging.rs#L15-L42\">日志宏</a> 与 \
                    <a href=\"docs/design/logging_design.md\">日志设计</a></p>";
        let out = post_process_doc_links(html, "https://github.com/o/r/blob/main", None);
        assert!(out.contains(
            "data-repo-href=\"https://github.com/o/r/blob/main/src/pkg/logging.rs#L15-L42\""
        ));
        assert!(out.contains(
            "data-repo-href=\"https://github.com/o/r/blob/main/docs/design/logging_design.md\""
        ));
        assert!(out.contains("class=\"doc-link-intercept\""));
        // 原 href 保留作降级
        assert!(out.contains("href=\"src/pkg/logging.rs#L15-L42\""));
    }

    #[test]
    fn external_link_gets_target_blank() {
        let html = "<a href=\"https://docs.rs/sqlx\">sqlx</a> <a href=\"mailto:a@b.c\">mail</a>";
        let out = post_process_doc_links(html, "https://github.com/o/r/blob/main", None);
        assert!(out.contains(
            "<a href=\"https://docs.rs/sqlx\" target=\"_blank\" rel=\"noopener noreferrer\">"
        ));
        assert!(out.contains("target=\"_blank\" rel=\"noopener noreferrer\">mail"));
        assert!(!out.contains("data-repo-href"));
        // 已有 target 则不动
        let keep = "<a href=\"https://example.com\" target=\"_top\">t</a>";
        assert_eq!(post_process_doc_links(keep, "base", None), keep);
    }

    #[test]
    fn chinese_content_not_corrupted() {
        let html = "<p>多 Agent 协作框架：组织与权限 🎯</p>\n\
                    <a href=\"docs/plan/日志管理重构.md\">日志重构计划</a>\n\
                    <p>中文「引号」与 emoji 🎨 混排段落</p>";
        let out = post_process_doc_links(html, BLOB_BASE, None);
        // 无 U+FFFD 替换字符（多字节字符未被字节级截断）
        assert!(!out.contains('\u{FFFD}'));
        // 中文段落与链接文本完整保留
        assert!(out.contains("多 Agent 协作框架：组织与权限 🎯"));
        assert!(out.contains("日志重构计划"));
        assert!(out.contains("中文「引号」与 emoji 🎨 混排段落"));
        // 中文路径链接被正确预拼（PlanDoc → blob_base + 原路径）
        assert!(out.contains(&format!(
            "data-repo-href=\"{BLOB_BASE}/docs/plan/日志管理重构.md\""
        )));
    }

    #[test]
    fn invalid_link_kept_as_is() {
        // classify 结果 Invalid（空 href）→ 原样保留
        let html = "<a href=\"\">空链接</a>";
        assert_eq!(
            post_process_doc_links(html, "https://github.com/o/r/blob/main", None),
            html
        );
    }

    #[test]
    fn diagram_html_opens_in_site() {
        // 架构图是自包含 HTML：站内新标签直开，不跳 GitHub 看源码
        let html = "<a href=\"docs/diagrams/ai-orz-architecture.html\">架构图</a>";
        let out = post_process_doc_links(html, "https://github.com/o/r/blob/main", None);
        assert!(out.contains("href=\"/docs/diagrams/ai-orz-architecture.html\""));
        assert!(out.contains("target=\"_blank\""));
        assert!(!out.contains("data-repo-href"));
    }

    #[test]
    fn dot_relative_link_switches_in_site() {
        // 源文 ./README.md 相对 docs/AGENT_GUIDE.md → docs/README.md，命中收录映射
        let context = ctx(
            "docs/AGENT_GUIDE.md",
            &[
                ("docs/README.md", "README.md"),
                ("docs/CODE_STANDARDS.md", "modules/abc.md"),
            ],
        );
        let html = "<a href=\"./README.md\">知识库指南</a> \
                    <a href=\"./CODE_STANDARDS.md\">编码规范</a>";
        let out = post_process_doc_links(html, BLOB_BASE, Some(&context));
        assert!(out.contains("data-doc-path=\"README.md\""));
        assert!(out.contains("data-doc-path=\"modules/abc.md\""));
        assert!(out.contains("data-doc-repo=\"docs/CODE_STANDARDS.md\""));
        assert!(out.contains("class=\"doc-link-internal\""));
        assert!(!out.contains("doc-link-intercept"));
        // href 降级仍是 GitHub 绝对地址
        assert!(
            out.contains(
                "href=\"https://github.com/runningcoders/ai_orz/blob/main/docs/README.md\""
            )
        );
    }

    #[test]
    fn uncollected_doc_falls_back_to_github() {
        // 未收录（如 archive）→ 远程 GitHub
        let context = ctx("docs/AGENT_GUIDE.md", &[("docs/README.md", "README.md")]);
        let html = "<a href=\"./archive/plan-archive/old.md\">旧方案</a>";
        let out = post_process_doc_links(html, BLOB_BASE, Some(&context));
        assert!(out.contains("class=\"doc-link-intercept\""));
        assert!(out.contains("archive/plan-archive/old.md"));
        assert!(!out.contains("doc-link-internal"));
    }

    #[test]
    fn percent_encoded_repo_path_matches_map() {
        // pulldown-cmark 会把中文路径编码，映射键是文件系统原始名
        let context = ctx(
            "docs/modules/11-前端应用/C-知识/Markdown渲染组件.md",
            &[(
                "docs/modules/11-前端应用/C-知识/知识图谱可视化.md",
                "modules/xyz.md",
            )],
        );
        let encoded = "<a href=\"./%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%E5%8F%AF%E8%A7%86%E5%8C%96.md\">图谱</a>";
        let out = post_process_doc_links(encoded, BLOB_BASE, Some(&context));
        assert!(out.contains("data-doc-path=\"modules/xyz.md\""));
        assert!(
            out.contains("data-doc-repo=\"docs/modules/11-前端应用/C-知识/知识图谱可视化.md\"")
        );
    }
}
