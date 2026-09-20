//! FTS5 全文搜索工具模块
//!
//! 提供 SQLite FTS5 全文搜索相关的通用工具函数，
//! 供各 DAO 层复用，避免 DAO 之间的互相依赖。
//!
//! ## 搜索计划（build_fts5_search_plan）
//!
//! trigram 分词器下 MATCH 短语要求「文档存在输入的连续子串」，且短于 3 字符的
//! 查询词无法形成任何 trigram token（零命中）。因此搜索词按空白/标点拆分为词元，
//! 并按长度路由到两条互斥的安全查询路径：
//!
//! - 词元 >= 3 字符 → MATCH 短语 OR 组合（`"rust" OR "tokio"`），BM25 可排序
//! - 词元 < 3 字符  → FTS 表列 LIKE 兜底（`node_name LIKE '%图谱%'`），BM25 不可用，
//!   需改用主表 `updated_at DESC` 排序
//!
//! 两条路径**永不混合在一条 SQL 中**（rank 在无 MATCH 上下文时行为不可依赖），
//! 混合词元场景由 DAO 分别执行后按主键合并去重（见 [merge_fts5_results]）。

/// 转义 FTS5 MATCH 关键词，封装为短语匹配
///
/// 将用户输入的关键词转义后用双引号包裹，作为短语匹配（phrase match），
/// 不会把空格解释为 AND 操作符。
///
/// 例如：`hello"world` -> `"hello""world"`
pub fn escape_fts5_keyword(keyword: &str) -> String {
    if keyword.trim().is_empty() {
        return String::new();
    }
    let escaped = keyword.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

/// 转义 LIKE 通配符并包裹为 `%term%` 包含模式
///
/// 用户输入中的 `\` `%` `_` 一律转义为字面量，配合 SQL 中的 `ESCAPE '\'` 子句使用。
pub fn escape_like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

/// 词元分隔符：空白 + 常见中英文标点
///
/// 保守集合：`.` `-` `+` `_` `@` `#` 等不出现在分隔符中，
/// 以保留 `node.js` / `rust-fmt` / `c++` 这类技术词的完整性。
const TERM_SEPARATORS: &[char] = &[
    ' ', '\t', '\r', '\n', '\u{3000}', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '/',
    '|', '&', '=', '，', '。', '、', '；', '：', '！', '？', '（', '）', '「', '」', '『', '』',
    '【', '】', '《', '》',
];

/// trigram 分词器可形成 token 的最小词元长度（unicode 字符数）
///
/// 短于此长度的词元在 MATCH 下零命中，必须路由到 LIKE 兜底路径。
const TRIGRAM_MIN_TERM_CHARS: usize = 3;

/// 将用户输入拆分为搜索词元（按空白与常见中英文标点切分，去空、保序、去重）
pub fn split_search_terms(keyword: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for part in keyword.split(TERM_SEPARATORS) {
        let term = part.trim();
        if term.is_empty() {
            continue;
        }
        if !terms.iter().any(|t| t == term) {
            terms.push(term.to_string());
        }
    }
    terms
}

/// FTS5 搜索计划：按词元长度路由后的两条互斥查询路径的参数
///
/// - `match_expr` 非空：DAO 应执行纯 MATCH 查询（表达式已 OR 组合，参数绑定为单值），
///   可用 `ORDER BY {fts_table}.rank` 做 BM25 排序
/// - `like_patterns` 非空：DAO 应执行纯 LIKE 兜底查询（配合
///   [build_fts5_like_clause] 生成 WHERE 片段），BM25 不可用，改用主表
///   `updated_at DESC` 排序
///
/// 两者可同时非空（混合词元），DAO 分别执行后用 [merge_fts5_results] 合并。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fts5SearchPlan {
    /// >= 3 字符词元转义后的 MATCH 表达式（`"rust" OR "tokio"`）；None 表示无长词元
    pub match_expr: Option<String>,
    /// < 3 字符词元转义后的 LIKE 模式（已含 % 包裹）；空表示无短词元
    pub like_patterns: Vec<String>,
}

/// 构建搜索计划：拆词元 → 按长度路由到 MATCH / LIKE 两条路径
///
/// 输入为空或全部分隔符时返回 None（调用方应直接返回空结果）。
pub fn build_fts5_search_plan(keyword: &str) -> Option<Fts5SearchPlan> {
    let terms = split_search_terms(keyword);
    if terms.is_empty() {
        return None;
    }

    let mut match_parts: Vec<String> = Vec::new();
    let mut like_patterns: Vec<String> = Vec::new();
    for term in &terms {
        if term.chars().count() >= TRIGRAM_MIN_TERM_CHARS {
            match_parts.push(escape_fts5_keyword(term));
        } else {
            like_patterns.push(escape_like_pattern(term));
        }
    }

    let match_expr = if match_parts.is_empty() {
        None
    } else {
        Some(match_parts.join(" OR "))
    };
    if match_expr.is_none() && like_patterns.is_empty() {
        return None;
    }

    Some(Fts5SearchPlan {
        match_expr,
        like_patterns,
    })
}

/// 构建 LIKE 兜底 WHERE 片段与绑定参数
///
/// 生成 `{fts_table}.{column} LIKE ? ESCAPE '\'` 的 OR 展开条件；
/// 参数顺序与条件顺序一致（按词元外层 × 列内层展开），DAO 按序绑定即可。
///
/// 返回 None 表示无可用条件（列或模式为空）。
///
/// ⚠️ 仅适用于 `query_as` + `format!` 组装 SQL 的消费方（文本 `?` 由
/// `Query::bind` 位置填充）。QueryBuilder 消费方必须改用
/// [`push_fts5_like_conditions`] —— 见其文档说明。
pub fn build_fts5_like_clause(
    fts_table: &str,
    columns: &[&str],
    like_patterns: &[String],
) -> Option<(String, Vec<String>)> {
    if columns.is_empty() || like_patterns.is_empty() {
        return None;
    }

    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    for pattern in like_patterns {
        for column in columns {
            conditions.push(format!("{}.{} LIKE ? ESCAPE '\\'", fts_table, column));
            params.push(pattern.clone());
        }
    }

    Some((conditions.join(" OR "), params))
}

/// 向 QueryBuilder 推送短词元 LIKE 兜底条件（`列 LIKE ? ESCAPE '\'` 的 OR 组合）
///
/// 与 [`build_fts5_like_clause`] 的区别：本函数面向 `sqlx::QueryBuilder` 消费方。
/// QueryBuilder 的 `push` 子句文本里的 `?` **不会**被 `push_bind` 的值消费——
/// `push_bind` 会额外追加一个新占位符。若把「子句文本 + 参数」两段式拼进
/// Builder（先 push 含 `?` 的文本、再 push_bind 参数），会产生 `...)???`
/// 这类悬空占位符，SQLite 直接 `near "?": syntax error`。
/// 条件文本与绑定值必须在本函数内交错推送，占位符与值一一对应。
///
/// 调用方需保证 columns / like_patterns 非空（本函数不输出空括号守卫）。
pub fn push_fts5_like_conditions(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>,
    table: &str,
    columns: &[&str],
    like_patterns: &[String],
) {
    for (i, pattern) in like_patterns.iter().enumerate() {
        for (j, column) in columns.iter().enumerate() {
            if i + j > 0 {
                builder.push(" OR ");
            }
            builder.push(format!("{}.{} LIKE ", table, column));
            builder.push_bind(pattern.clone());
            builder.push(" ESCAPE '\\'");
        }
    }
}

/// 合并 MATCH 主结果与 LIKE 兜底结果（按主键去重，兜底结果仅补位，截断至 limit）
///
/// 主结果（BM25 排序）在前保留原始顺序；兜底结果（updated_at 排序）中
/// 未出现过的新条目追加在尾部。
pub fn merge_fts5_results<T, K>(
    primary: Vec<(T, Option<f32>)>,
    secondary: Vec<(T, Option<f32>)>,
    key_of: impl Fn(&T) -> K,
    limit: usize,
) -> Vec<(T, Option<f32>)>
where
    K: std::hash::Hash + Eq,
{
    let mut seen = std::collections::HashSet::new();
    let mut merged: Vec<(T, Option<f32>)> = Vec::with_capacity(primary.len() + secondary.len());
    for (row, rank) in primary.into_iter().chain(secondary) {
        let key = key_of(&row);
        if seen.insert(key) {
            merged.push((row, rank));
            if merged.len() >= limit {
                break;
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
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
}
