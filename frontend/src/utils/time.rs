//! 时间格式化工具

/// 当前毫秒时间戳
///
/// 注意：wasm32-unknown-unknown 下 `std::time::SystemTime::now()` 未实现
/// （`library/std/src/sys/time/unsupported.rs` 直接 panic：
/// "time not implemented on this platform"），必须走 `js_sys::Date::now()`。
/// 历史上这里用 SystemTime，导致发送消息/生成临时 ID 时崩溃。
pub fn now_ms() -> i64 {
    js_sys::Date::now() as i64
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

/// `<input type="datetime-local">` 输入值（"YYYY-MM-DDTHH:MM"）→ unix 毫秒
///
/// 输入按**本地时区**解释；空串 / 无法解析返回 None。
pub fn parse_datetime_local_to_ms(s: &str) -> Option<i64> {
    use chrono::{Local, NaiveDateTime, TimeZone};
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok()?;
    Local
        .from_local_datetime(&ndt)
        .single()
        .map(|dt| dt.timestamp_millis())
}

/// unix 毫秒 → `<input type="datetime-local">` 可用的 "YYYY-MM-DDTHH:MM"（本地时区）
pub fn ms_to_datetime_local_value(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts_ms / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}
