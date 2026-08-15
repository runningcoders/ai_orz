//! 渲染期链接后处理：站内相对路径 `<a>` 预拼 data-repo-href（GitHub blob 绝对 URL）
//!
//! 配合 index.html 全局点击拦截 JS 桥（`a.doc-link-intercept`）实现文档中心
//! 站内链接正确分发（AGENTS §2.1.2）；原 href 保留，JS 桥失效时降级为默认导航。
//!
//! 按 chars 迭代推进 + is_char_boundary 锚点检查，保证中文（多字节 UTF-8）不被拆坏。

use common::doc_link::DocLinkClassifier;

/// GitHub blob 前缀（V1 全部站内链接走 GitHub 新窗口）
// 与 git remote origin（git@github.com:runningcoders/ai_orz.git）对齐；换 remote 时同步改
pub const BLOB_BASE: &str = "https://github.com/runningcoders/ai_orz/blob/main";

/// 对 pulldown-cmark 产物 HTML 做链接后处理：
/// - 外链（http / # / mailto 开头）补 `target="_blank" rel="noopener noreferrer"`（已有 target 则不动）
/// - 站内相对路径链接用 DocLinkClassifier 生成 GitHub 绝对 URL，
///   追加 `data-repo-href="..."` + `class="doc-link-intercept"`；classify 结果 Invalid 则原样保留
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
                    let target = DocLinkClassifier::classify(href);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_relative_link_gets_repo_href() {
        let html = "<p>见 <a href=\"src/pkg/logging.rs#L15-L42\">日志宏</a> 与 \
                    <a href=\"docs/design/logging_design.md\">日志设计</a></p>";
        let out = post_process_doc_links(html, "https://github.com/o/r/blob/main");
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
        let out = post_process_doc_links(html, "https://github.com/o/r/blob/main");
        assert!(out.contains(
            "<a href=\"https://docs.rs/sqlx\" target=\"_blank\" rel=\"noopener noreferrer\">"
        ));
        assert!(out.contains("target=\"_blank\" rel=\"noopener noreferrer\">mail"));
        assert!(!out.contains("data-repo-href"));
        // 已有 target 则不动
        let keep = "<a href=\"https://example.com\" target=\"_top\">t</a>";
        assert_eq!(post_process_doc_links(keep, "base"), keep);
    }

    #[test]
    fn chinese_content_not_corrupted() {
        let html = "<p>多 Agent 协作框架：组织与权限 🎯</p>\n\
                    <a href=\"docs/plan/日志管理重构.md\">日志重构计划</a>\n\
                    <p>中文「引号」与 emoji 🎨 混排段落</p>";
        let out = post_process_doc_links(html, BLOB_BASE);
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
            post_process_doc_links(html, "https://github.com/o/r/blob/main"),
            html
        );
    }
}
