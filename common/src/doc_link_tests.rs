//! tests 单元测试（拆分自 doc_link.rs）
//!
//! 文件瘦身：原 460 行 → 286 行，测试体 175 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
    let r = DocLinkClassifier::classify("docs/wiki/zh/content/功能模块/系统管理/日志管理系统.md");
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
    let r =
        DocLinkClassifier::classify("docs/wiki/knowledge/zh/工具系统/CoreTool%20trait%20三层.md");
    assert!(matches!(r, DocLinkTarget::RagCard { .. }));
}

// —— 导入期相对链接归一化 ——

#[test]
fn t20_normalize_dot_slash_from_doc_root() {
    assert_eq!(
        normalize_repo_relative("./CODE_STANDARDS.md", "docs/AGENT_GUIDE.md"),
        "docs/CODE_STANDARDS.md"
    );
    assert_eq!(
        normalize_repo_relative("./modules/INDEX.md", "docs/AGENT_GUIDE.md"),
        "docs/modules/INDEX.md"
    );
}

#[test]
fn t21_normalize_dot_dot_into_sibling_tier() {
    // C-知识 → 同模块的 A-设计
    assert_eq!(
        normalize_repo_relative(
            "../A-设计/runtime_design.md",
            "docs/modules/02-Agent运行时/C-知识/运行时领域.md"
        ),
        "docs/modules/02-Agent运行时/A-设计/runtime_design.md"
    );
    // 多级回退：卡目录 → 仓库根
    assert_eq!(
        normalize_repo_relative(
            "../../../../../AGENTS.md",
            "docs/modules/02-Agent运行时/C-知识/卡/卡.md"
        ),
        "AGENTS.md"
    );
}

#[test]
fn t22_normalize_keeps_fragment_and_query() {
    assert_eq!(
        normalize_repo_relative("../A-设计/x_design.md#L10-L20", "docs/modules/01/a/b.md"),
        "docs/modules/01/A-设计/x_design.md#L10-L20"
    );
    assert_eq!(
        normalize_repo_relative("./x.md?plain=1", "docs/a.md"),
        "docs/x.md?plain=1"
    );
}

#[test]
fn t23_normalize_passthrough_and_overflow() {
    // 非相对路径原样返回
    for href in [
        "https://github.com/o/r",
        "#anchor",
        "mailto:a@b.c",
        "docs/modules/01/a.md",
        "",
    ] {
        assert_eq!(normalize_repo_relative(href, "docs/a.md"), href);
    }
    // 恰好弹到仓库根 → 归一化为根级路径
    assert_eq!(normalize_repo_relative("../x.md", "docs/a.md"), "x.md");
    assert_eq!(
        normalize_repo_relative("../../x.md", "docs/sub/a.md"),
        "x.md"
    );
    // 越过仓库根（基准目录在根，没有可弹的层级）→ 原样返回，不猜
    assert_eq!(normalize_repo_relative("../x.md", "AGENTS.md"), "../x.md");
}

#[test]
fn t24_rewrite_markdown_links() {
    let md = "见 [编码规范](./CODE_STANDARDS.md) 与 [运行时](./modules/02-Agent运行时/A-设计/runtime_design.md#L1-L10)。\n";
    let out = rewrite_markdown_relative_links(md, "docs/AGENT_GUIDE.md");
    assert!(out.contains("[编码规范](docs/CODE_STANDARDS.md)"));
    assert!(out.contains("[运行时](docs/modules/02-Agent运行时/A-设计/runtime_design.md#L1-L10)"));
}

#[test]
fn t25_rewrite_keeps_code_examples() {
    let md = "\
```markdown
[示例](./x.md)
```

行内 `[示例](../y.md)` 也不动，但外面 [真链接](./real.md) 要改。
";
    let out = rewrite_markdown_relative_links(md, "docs/a.md");
    // 围栏内原样
    assert!(out.contains("[示例](./x.md)"));
    // 行内代码原样
    assert!(out.contains("`[示例](../y.md)`"));
    // 代码外的真链接被归一化
    assert!(out.contains("[真链接](docs/real.md)"));
}

#[test]
fn t26_rewrite_angle_bracket_path_with_parens() {
    // 含 () 的目录必须用 <...> 包裹；同模块 C-知识 → A-设计 上跳一层
    let md = "[AOP](<../A-设计/aop_design.md>)\n";
    let out = rewrite_markdown_relative_links(md, "docs/modules/10-异步基建(AOP)/C-知识/x.md");
    assert_eq!(
        out.trim_end(),
        "[AOP](<docs/modules/10-异步基建(AOP)/A-设计/aop_design.md>)"
    );
}

#[test]
fn t27_rewrite_is_idempotent() {
    let md = "[a](./x.md) [b](../y.md)\n";
    let once = rewrite_markdown_relative_links(md, "docs/sub/a.md");
    let twice = rewrite_markdown_relative_links(&once, "docs/sub/a.md");
    assert_eq!(once, twice);
}
