//! tests 单元测试（拆分自 log_query.rs）
//!
//! 文件瘦身：原 724 行 → 345 行，测试体 380 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 构造一条标准 tracing-subscriber json 格式日志的 `serde_json::Value`
fn make_log_json(
    timestamp: &str,
    level: &str,
    message: &str,
    log_id: Option<&str>,
    user_id: Option<&str>,
    operation: Option<&str>,
) -> serde_json::Value {
    let mut fields = serde_json::json!({ "message": message });
    if let Some(id) = log_id {
        fields["log_id"] = serde_json::Value::String(id.to_string());
    }
    if let Some(uid) = user_id {
        fields["user_id"] = serde_json::Value::String(uid.to_string());
    }
    if let Some(op) = operation {
        fields["operation"] = serde_json::Value::String(op.to_string());
    }
    serde_json::json!({
        "timestamp": timestamp,
        "level": level,
        "fields": fields,
        "target": "ai_orz::pkg",
        "filename": "src/pkg/mod.rs",
        "line_number": 35,
    })
}

/// 测试 `parse_timestamp_to_millis`：合法 RFC3339 时间戳能正确解析为毫秒
#[test]
fn test_parse_timestamp_to_millis_valid() {
    // 1970-01-01T00:00:00Z = 0 ms
    assert_eq!(parse_timestamp_to_millis("1970-01-01T00:00:00Z"), Some(0));

    // 1970-01-01T00:00:01Z = 1000 ms
    assert_eq!(
        parse_timestamp_to_millis("1970-01-01T00:00:01Z"),
        Some(1000)
    );

    // 含毫秒和时区：2026-07-17T12:34:56.789Z
    let ms = parse_timestamp_to_millis("2026-07-17T12:34:56.789Z");
    assert!(ms.is_some());
    assert!(ms.unwrap() > 0);
}

/// 测试 `parse_timestamp_to_millis`：非法时间戳返回 None
#[test]
fn test_parse_timestamp_to_millis_invalid() {
    assert_eq!(parse_timestamp_to_millis("not a timestamp"), None);
    assert_eq!(parse_timestamp_to_millis(""), None);
    assert_eq!(parse_timestamp_to_millis("2026-07-17 12:34:56"), None); // 非 RFC3339
}

/// 测试 `parse_and_filter`：无任何过滤条件时应返回解析后的 entry
#[test]
fn test_parse_and_filter_no_filters() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "INFO",
        "hello world",
        Some("req-123"),
        Some("user-1"),
        Some("create_user"),
    );

    let entry = parse_and_filter(&raw, None, None, None, None, None).expect("应解析出 LogEntry");
    assert_eq!(entry.timestamp, "2026-07-17T12:00:00Z");
    assert_eq!(entry.level, "INFO");
    assert_eq!(entry.message, "hello world");
    assert_eq!(entry.log_id.as_deref(), Some("req-123"));
    assert_eq!(entry.user_id.as_deref(), Some("user-1"));
    assert_eq!(entry.operation.as_deref(), Some("create_user"));
}

/// 测试 `parse_and_filter`：关键字过滤（message 不区分大小写包含）
#[test]
fn test_parse_and_filter_keyword() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "INFO",
        "User logged in successfully",
        None,
        None,
        None,
    );

    // 大小写不敏感，匹配 "USER"
    assert!(parse_and_filter(&raw, Some("user"), None, None, None, None).is_some());
    // 完全无关的关键字
    assert!(parse_and_filter(&raw, Some("nonexistent"), None, None, None, None).is_none());
    // 空关键字：包含空串恒为真（边界情况）
    assert!(parse_and_filter(&raw, Some(""), None, None, None, None).is_some());
}

/// 测试 `parse_and_filter`：日志级别过滤
///
/// 注意：`parse_and_filter` 接收的 `level_filter` 必须已经是大写形式
/// （实际调用方 `query_logs` 会先做 `.to_uppercase()`）。这里模拟调用方行为。
#[test]
fn test_parse_and_filter_level() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "ERROR",
        "something failed",
        None,
        None,
        None,
    );

    // 大写过滤匹配大写 level
    assert!(parse_and_filter(&raw, None, None, Some("ERROR"), None, None).is_some());
    // 小写输入经 to_uppercase 后也应匹配（模拟 query_logs 预处理）
    let upper_error = "error".to_uppercase();
    assert!(parse_and_filter(&raw, None, None, Some(&upper_error), None, None).is_some());
    // 不匹配其他级别
    let upper_info = "INFO".to_uppercase();
    assert!(parse_and_filter(&raw, None, None, Some(&upper_info), None, None).is_none());
    let upper_warn = "WARN".to_uppercase();
    assert!(parse_and_filter(&raw, None, None, Some(&upper_warn), None, None).is_none());

    // 原始 level 字段是小写时也应能匹配大写过滤（函数内部会 to_uppercase level）
    let raw_lower = make_log_json(
        "2026-07-17T12:00:00Z",
        "info",
        "low level msg",
        None,
        None,
        None,
    );
    assert!(parse_and_filter(&raw_lower, None, None, Some("INFO"), None, None).is_some());
    assert!(parse_and_filter(&raw_lower, None, None, Some("ERROR"), None, None).is_none());
}

/// 测试 `parse_and_filter`：log_id 精确匹配
#[test]
fn test_parse_and_filter_log_id() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "INFO",
        "msg",
        Some("req-abc-001"),
        None,
        None,
    );

    // 精确匹配
    assert!(parse_and_filter(&raw, None, Some("req-abc-001"), None, None, None).is_some());
    // 不匹配其他 log_id
    assert!(parse_and_filter(&raw, None, Some("req-abc-002"), None, None, None).is_none());
    // 部分匹配不算
    assert!(parse_and_filter(&raw, None, Some("req-abc"), None, None, None).is_none());
}

/// 测试 `parse_and_filter`：时间范围过滤
#[test]
fn test_parse_and_filter_time_range() {
    // 2026-07-17T12:00:00Z 的 unix 毫秒
    let ts = parse_timestamp_to_millis("2026-07-17T12:00:00Z").unwrap();
    let raw = make_log_json("2026-07-17T12:00:00Z", "INFO", "msg", None, None, None);

    // 起始时间 <= ts：应通过
    assert!(parse_and_filter(&raw, None, None, None, Some(ts - 1000), None).is_some());
    // 起始时间 == ts：应通过（含）
    assert!(parse_and_filter(&raw, None, None, None, Some(ts), None).is_some());
    // 起始时间 > ts：不通过
    assert!(parse_and_filter(&raw, None, None, None, Some(ts + 1000), None).is_none());

    // 结束时间 >= ts：应通过
    assert!(parse_and_filter(&raw, None, None, None, None, Some(ts + 1000)).is_some());
    // 结束时间 == ts：应通过（含）
    assert!(parse_and_filter(&raw, None, None, None, None, Some(ts)).is_some());
    // 结束时间 < ts：不通过
    assert!(parse_and_filter(&raw, None, None, None, None, Some(ts - 1000)).is_none());
}

/// 测试 `parse_and_filter`：当 log_id/user_id/operation 字段为空字符串时，
/// 应被 `filter(|s| !s.is_empty())` 过滤为 None
#[test]
fn test_parse_and_filter_empty_fields_become_none() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "INFO",
        "msg",
        Some(""), // 空 log_id
        Some(""), // 空 user_id
        Some(""), // 空 operation
    );

    let entry = parse_and_filter(&raw, None, None, None, None, None).expect("应解析出 LogEntry");
    assert_eq!(entry.log_id, None);
    assert_eq!(entry.user_id, None);
    assert_eq!(entry.operation, None);
}

/// 测试 `parse_and_filter`：组合过滤（keyword + level + log_id 同时生效）
#[test]
fn test_parse_and_filter_combined() {
    let raw = make_log_json(
        "2026-07-17T12:00:00Z",
        "WARN",
        "Disk almost full",
        Some("req-combo"),
        None,
        None,
    );

    // 三个条件全满足
    assert!(
        parse_and_filter(
            &raw,
            Some("disk"),
            Some("req-combo"),
            Some("WARN"),
            None,
            None,
        )
        .is_some()
    );

    // 任意一个不满足都应返回 None
    assert!(
        parse_and_filter(
            &raw,
            Some("network"),
            Some("req-combo"),
            Some("WARN"),
            None,
            None
        )
        .is_none()
    );
    assert!(
        parse_and_filter(
            &raw,
            Some("disk"),
            Some("wrong-id"),
            Some("WARN"),
            None,
            None
        )
        .is_none()
    );
    assert!(
        parse_and_filter(
            &raw,
            Some("disk"),
            Some("req-combo"),
            Some("ERROR"),
            None,
            None
        )
        .is_none()
    );
}

/// 测试 `parse_and_filter`：当 timestamp 非法且启用了时间过滤时返回 None
#[test]
fn test_parse_and_filter_invalid_timestamp_with_time_filter() {
    let mut raw = make_log_json("not-a-timestamp", "INFO", "msg", None, None, None);
    // 保留 raw 但确保 timestamp 是非法的
    raw["timestamp"] = serde_json::Value::String("not-a-timestamp".to_string());

    // 无时间过滤时仍能解析（timestamp 字段保留原值）
    let entry = parse_and_filter(&raw, None, None, None, None, None)
        .expect("无时间过滤时不应被 timestamp 合法性影响");
    assert_eq!(entry.timestamp, "not-a-timestamp");

    // 启用时间过滤时，因无法解析 timestamp 返回 None
    assert!(parse_and_filter(&raw, None, None, None, Some(0), None).is_none());
}

/// 测试 `collect_log_files`：仅收集符合 `ai_orz.log.YYYY-MM-DD` 命名
/// 且在最近 MAX_SCAN_DAYS 天内的文件
#[test]
fn test_collect_log_files_filters_by_name_and_date() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // 1) 今天的日志文件（应被收集）
    let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let today_name = format!("{}{}", LOG_FILE_PREFIX, today);
    std::fs::write(dir.path().join(&today_name), b"{}").expect("write today log");

    // 2) 命名不匹配的文件（应被忽略）
    std::fs::write(dir.path().join("random.log"), b"{}").expect("write random");
    std::fs::write(dir.path().join("ai_orz.log"), b"{}").expect("write no-date");
    std::fs::write(dir.path().join("ai_orz.log.not-a-date"), b"{}").expect("write bad-date");

    // 3) 一个子目录（应被忽略，因为 collect_log_files 仅收集 is_file）
    std::fs::create_dir_all(dir.path().join("ai_orz.log.2026-01-01")).expect("mkdir disguised");

    let files = collect_log_files(dir.path());
    // 至少应包含今天的文件
    assert!(
        files.iter().any(|f| f.ends_with(&today_name)),
        "应收集今天的日志文件, 实际: {:?}",
        files
    );
    // 不应包含 random.log / ai_orz.log / ai_orz.log.not-a-date
    assert!(
        !files.iter().any(|f| f.ends_with("random.log")),
        "不应收集 random.log, 实际: {:?}",
        files
    );
    assert!(
        !files
            .iter()
            .any(|f| f.ends_with("ai_orz.log") && !f.ends_with(&today_name)),
        "不应收集无日期后缀的 ai_orz.log, 实际: {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("ai_orz.log.not-a-date")),
        "不应收集非法日期后缀, 实际: {:?}",
        files
    );
}

/// 测试 `collect_log_files`：超过 MAX_SCAN_DAYS 天的旧文件应被排除
#[test]
fn test_collect_log_files_excludes_old_files() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // 构造一个明确超出 MAX_SCAN_DAYS 的旧日期
    let old_date = Utc::now().date_naive() - chrono::Duration::days(MAX_SCAN_DAYS + 5);
    let old_name = format!("{}{}", LOG_FILE_PREFIX, old_date.format("%Y-%m-%d"));
    std::fs::write(dir.path().join(&old_name), b"{}").expect("write old log");

    let files = collect_log_files(dir.path());
    assert!(
        !files.iter().any(|f| f.ends_with(&old_name)),
        "超过 MAX_SCAN_DAYS 天的旧文件应被排除, 实际: {:?}",
        files
    );
}

/// 测试 `collect_log_files`：目录不存在时返回空 Vec（不 panic）
#[test]
fn test_collect_log_files_nonexistent_dir() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let nonexistent = dir.path().join("does-not-exist");
    let files = collect_log_files(&nonexistent);
    assert!(files.is_empty());
}

/// 测试 `LogQuery` 与 `QueryLogsResponse` 结构基本字段
/// （验证默认构造与字段可访问性，作为字段级单元测试）
#[test]
fn test_log_query_struct_fields() {
    let q = LogQuery {
        keyword: Some("foo".to_string()),
        log_id: Some("req-1".to_string()),
        level: Some("INFO".to_string()),
        start_time: Some(1000),
        end_time: Some(2000),
        page: 1,
        page_size: 20,
    };
    assert_eq!(q.keyword.as_deref(), Some("foo"));
    assert_eq!(q.log_id.as_deref(), Some("req-1"));
    assert_eq!(q.level.as_deref(), Some("INFO"));
    assert_eq!(q.start_time, Some(1000));
    assert_eq!(q.end_time, Some(2000));
    assert_eq!(q.page, 1);
    assert_eq!(q.page_size, 20);

    let r = QueryLogsResponse {
        total: 0,
        entries: Vec::new(),
        page: 1,
        page_size: 20,
    };
    assert_eq!(r.total, 0);
    assert!(r.entries.is_empty());
}
