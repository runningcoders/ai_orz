//! 时间格式化工具

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前毫秒时间戳
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 毫秒时间戳 → "HH:MM"（本地时区，解析失败返回 "--:--"）
pub fn format_time_hm(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts_ms / 1000, 0) {
        chrono::LocalResult::Single(dt) => format!("{}", dt.format("%H:%M")),
        _ => "--:--".to_string(),
    }
}

/// 毫秒时间戳 → "YYYY-MM-DD HH:MM"（本地时区，解析失败回退为原始值）
pub fn format_datetime(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts_ms / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts_ms.to_string())
}

/// 毫秒时间戳 → "YYYY-MM-DD HH:MM:SS"（本地时区，解析失败回退为原始值）
pub fn format_datetime_full(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts_ms / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts_ms.to_string())
}

/// 可选毫秒时间戳 → "YYYY-MM-DD HH:MM:SS"，None 返回 "—"
pub fn format_timestamp_opt(ts_ms: Option<i64>) -> String {
    ts_ms
        .map(format_datetime_full)
        .unwrap_or_else(|| "—".to_string())
}

/// RFC3339/ISO8601 字符串 → "YYYY-MM-DD HH:MM:SS"（本地时区，解析失败原样返回）
pub fn format_rfc3339(ts: &str) -> String {
    if ts.is_empty() {
        return "-".to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if ts.len() >= 19 {
        return ts[..19].replace('T', " ");
    }
    ts.to_string()
}
