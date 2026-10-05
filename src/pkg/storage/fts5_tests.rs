//! tests 单元测试（拆分自 fts5.rs）
//!
//! 文件瘦身：原 387 行 → 214 行，测试体 174 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn test_escape_fts5_keyword() {
    // 空字符串
    assert_eq!(escape_fts5_keyword(""), "");
    assert_eq!(escape_fts5_keyword("   "), "");

    // 普通关键词
    assert_eq!(escape_fts5_keyword("hello"), "\"hello\"");
    assert_eq!(escape_fts5_keyword("rust"), "\"rust\"");

    // 含双引号的关键词：内部双引号双写
    assert_eq!(escape_fts5_keyword("hello\"world"), "\"hello\"\"world\"");

    // 含空格的关键词：作为短语匹配，空格不解释为 AND
    assert_eq!(escape_fts5_keyword("hello world"), "\"hello world\"");

    // 含 FTS5 特殊字符的关键词
    assert_eq!(escape_fts5_keyword("test*"), "\"test*\"");
    assert_eq!(escape_fts5_keyword("a(b)c"), "\"a(b)c\"");
}

#[test]
fn test_split_search_terms() {
    // 空输入 / 全分隔符
    assert!(split_search_terms("").is_empty());
    assert!(split_search_terms("  ,，。 ").is_empty());

    // 空白拆分
    assert_eq!(
        split_search_terms("rust 异步编程"),
        vec!["rust".to_string(), "异步编程".to_string()]
    );

    // 中文标点拆分
    assert_eq!(
        split_search_terms("向量，搜索。知识图谱"),
        vec![
            "向量".to_string(),
            "搜索".to_string(),
            "知识图谱".to_string()
        ]
    );

    // 技术词不被破坏
    assert_eq!(
        split_search_terms("node.js rust-fmt c++"),
        vec![
            "node.js".to_string(),
            "rust-fmt".to_string(),
            "c++".to_string()
        ]
    );

    // 去重保序
    assert_eq!(
        split_search_terms("rust rust tokio"),
        vec!["rust".to_string(), "tokio".to_string()]
    );

    // 英文标点拆分
    assert_eq!(
        split_search_terms("a/b|c&d=e"),
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string()
        ]
    );
}

#[test]
fn test_escape_like_pattern() {
    assert_eq!(escape_like_pattern("图谱"), "%图谱%");
    assert_eq!(escape_like_pattern("50%"), "%50\\%%");
    assert_eq!(escape_like_pattern("a_b"), "%a\\_b%");
    assert_eq!(escape_like_pattern("a\\b"), "%a\\\\b%");
}

#[test]
fn test_build_fts5_search_plan() {
    // 空输入
    assert_eq!(build_fts5_search_plan(""), None);
    assert_eq!(build_fts5_search_plan(" , "), None);

    // 纯长词元：只有 MATCH
    let plan = build_fts5_search_plan("rust tokio").unwrap();
    assert_eq!(plan.match_expr.as_deref(), Some("\"rust\" OR \"tokio\""));
    assert!(plan.like_patterns.is_empty());

    // 纯短词元：只有 LIKE（中文双字词）
    let plan = build_fts5_search_plan("知识 图谱").unwrap();
    assert_eq!(plan.match_expr, None);
    assert_eq!(
        plan.like_patterns,
        vec!["%知识%".to_string(), "%图谱%".to_string()]
    );

    // 混合词元：两条路径并存
    let plan = build_fts5_search_plan("rust 图谱").unwrap();
    assert_eq!(plan.match_expr.as_deref(), Some("\"rust\""));
    assert_eq!(plan.like_patterns, vec!["%图谱%".to_string()]);

    // 3 字符边界：恰好 3 字符走 MATCH
    let plan = build_fts5_search_plan("向量检索").unwrap();
    assert_eq!(plan.match_expr.as_deref(), Some("\"向量检索\""));

    // 特殊字符词元被安全转义
    let plan = build_fts5_search_plan("50% off").unwrap();
    assert_eq!(plan.match_expr.as_deref(), Some("\"50%\" OR \"off\""));
    assert!(plan.like_patterns.is_empty());
}

#[test]
fn test_build_fts5_like_clause() {
    // 正常构建：条件与参数顺序一致（词元外层 × 列内层）
    let (clause, params) = build_fts5_like_clause(
        "knowledge_node_fts",
        &["node_name", "summary"],
        &["%图谱%".to_string(), "%任务%".to_string()],
    )
    .unwrap();
    assert_eq!(
        clause,
        "knowledge_node_fts.node_name LIKE ? ESCAPE '\\' \
         OR knowledge_node_fts.summary LIKE ? ESCAPE '\\' \
         OR knowledge_node_fts.node_name LIKE ? ESCAPE '\\' \
         OR knowledge_node_fts.summary LIKE ? ESCAPE '\\'"
    );
    assert_eq!(
        params,
        vec![
            "%图谱%".to_string(),
            "%图谱%".to_string(),
            "%任务%".to_string(),
            "%任务%".to_string()
        ]
    );

    // 空输入
    assert_eq!(build_fts5_like_clause("t", &["c"], &[]), None);
    assert_eq!(build_fts5_like_clause("t", &[], &["%x%".to_string()]), None);
}

#[test]
fn test_merge_fts5_results() {
    let primary = vec![("a".to_string(), Some(1.0)), ("b".to_string(), Some(2.0))];
    let secondary = vec![
        ("b".to_string(), None),
        ("c".to_string(), None),
        ("d".to_string(), None),
    ];

    // 去重 + 补位 + 保序
    let merged = merge_fts5_results(primary.clone(), secondary.clone(), |s| s.clone(), 10);
    assert_eq!(
        merged,
        vec![
            ("a".to_string(), Some(1.0)),
            ("b".to_string(), Some(2.0)),
            ("c".to_string(), None),
            ("d".to_string(), None),
        ]
    );

    // limit 截断
    let merged = merge_fts5_results(primary, secondary, |s| s.clone(), 3);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].0, "a");
    assert_eq!(merged[2].0, "c");
}
