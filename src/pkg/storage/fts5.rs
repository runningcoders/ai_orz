//! FTS5 全文搜索工具模块
//!
//! 提供 SQLite FTS5 全文搜索相关的通用工具函数，
//! 供各 DAO 层复用，避免 DAO 之间的互相依赖。

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
}
