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

/// 毫秒时间戳 → 消息流分阶段时间（本地时区，解析失败返回 "--:--"）
///
/// 按消息日与「今天」的日历日距离分档：越近的时间越简短，远的时间补日期——
/// 今天 → "HH:MM"；昨天 → "昨天 HH:MM"；今年其他日 → "MM-DD HH:MM"；跨年 → "YYYY-MM-DD HH:MM"。
/// 注意「今天/昨天」按日历日判定而非 24 小时窗口：凌晨回看昨晚的消息要显示「昨天」。
pub fn format_message_time(ts_ms: i64) -> String {
    format_message_time_with_now(ts_ms, now_ms())
}

/// `format_message_time` 的可测内核：「现在」参数化，单测无需 mock 时钟
fn format_message_time_with_now(ts_ms: i64, now: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let Some(msg) = Local.timestamp_opt(ts_ms / 1000, 0).single() else {
        return "--:--".to_string();
    };
    let Some(now) = Local.timestamp_opt(now / 1000, 0).single() else {
        return format!("{}", msg.format("%H:%M"));
    };
    let msg_date = msg.date_naive();
    let today = now.date_naive();
    // 「昨天」优先于年份分档：1 月 1 日回看去年 12 月 31 日的消息，该显示「昨天」
    if msg_date == today {
        format!("{}", msg.format("%H:%M"))
    } else if Some(msg_date) == today.pred_opt() {
        format!("昨天 {}", msg.format("%H:%M"))
    } else if msg_date.year() == today.year() {
        format!("{}", msg.format("%m-%d %H:%M"))
    } else {
        format!("{}", msg.format("%Y-%m-%d %H:%M"))
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

#[cfg(test)]
mod tests {
    use super::format_message_time_with_now;
    use chrono::{Datelike, Local, NaiveDate, TimeZone};

    /// 各分支的样本时间都相对「真实现在」派生，测试在任何日期/时区运行都不漂移
    fn now_dt() -> chrono::DateTime<Local> {
        Local::now()
    }

    fn ms_at(d: NaiveDate, h: u32, m: u32) -> i64 {
        Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), h, m, 0)
            .single()
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn today_shows_hour_minute_only() {
        let now = now_dt();
        assert_eq!(
            format_message_time_with_now(ms_at(now.date_naive(), 9, 30), now.timestamp_millis()),
            "09:30"
        );
    }

    #[test]
    fn yesterday_prefixes_label() {
        let now = now_dt();
        let yesterday = now.date_naive().pred_opt().unwrap();
        assert_eq!(
            format_message_time_with_now(ms_at(yesterday, 23, 5), now.timestamp_millis()),
            "昨天 23:05"
        );
    }

    #[test]
    fn same_year_other_day_shows_month_day() {
        let now = now_dt();
        let today = now.date_naive();
        // 避开今天/昨天：1/1、1/2 往前找两天会落到去年，年初改用今年的 12/31（未来日也按日历日分档）
        let other_day = if today.ordinal() >= 3 {
            today.pred_opt().and_then(|d| d.pred_opt()).unwrap()
        } else {
            NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap()
        };
        assert_eq!(
            format_message_time_with_now(ms_at(other_day, 12, 0), now.timestamp_millis()),
            format!("{} 12:00", other_day.format("%m-%d"))
        );
    }

    #[test]
    fn previous_year_shows_full_date() {
        let now = now_dt();
        let today = now.date_naive();
        // 今天是 2/29 时 with_year 到非闰年会落空，回退到去年 3/1
        let last_year = today
            .with_year(today.year() - 1)
            .or_else(|| NaiveDate::from_ymd_opt(today.year() - 1, 3, 1))
            .unwrap();
        assert_eq!(
            format_message_time_with_now(ms_at(last_year, 8, 8), now.timestamp_millis()),
            format!("{} 08:08", last_year.format("%Y-%m-%d"))
        );
    }

    #[test]
    fn unparseable_timestamp_falls_back_to_placeholder() {
        let now = now_dt();
        assert_eq!(
            format_message_time_with_now(i64::MAX, now.timestamp_millis()),
            "--:--"
        );
    }
}
