//! Cron 表达式解析（纯算法，前后端共用）。

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use crate::error::{Result, err};

/// 解析 cron 表达式并计算从指定时间起的下一次触发时间（UTC 毫秒）。
///
/// - `expression`: 5 字段（Unix cron）或 6~7 字段（含秒/年）
/// - `timezone`: IANA 时区名，如 `Asia/Shanghai`
/// - `from`: 起始时刻（UTC）；结果是严格晚于它的第一次触发
pub fn next_run_at(expression: &str, timezone: &str, from: DateTime<Utc>) -> Result<i64> {
    let tz = parse_timezone(timezone)?;
    let schedule = parse_schedule(expression)?;
    let from_tz = from.with_timezone(&tz);
    let next = schedule.after(&from_tz).next().ok_or_else(|| {
        err!(
            InvalidRequest,
            "No future trigger time for cron expression '{}'",
            expression
        )
    })?;
    Ok(next.with_timezone(&Utc).timestamp_millis())
}

/// 校验 cron 表达式合法，且在给定时刻之后能产生触发时间。
///
/// `now` 由调用方显式传入，保持本模块纯函数、可在前端安全调用
/// （wasm32 下 `Utc::now()` 会走未实现的 `SystemTime`）。
pub fn validate_expression(expression: &str, timezone: &str, now: DateTime<Utc>) -> Result<()> {
    let tz = parse_timezone(timezone)?;
    let schedule = parse_schedule(expression)?;
    schedule
        .after(&now.with_timezone(&tz))
        .next()
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "No future trigger time for cron expression '{}'",
                expression
            )
        })?;
    Ok(())
}

/// 解析表达式为 `cron::Schedule`，统一 5 字段补秒的规范化口径。
///
/// 「下一次计算」与「区间展开」必须走同一套规范化，否则日历视图会
/// 算出与调度器不同的实例。
pub fn parse_schedule(expression: &str) -> Result<cron::Schedule> {
    let normalized = normalize_expression(expression);
    normalized.parse::<cron::Schedule>().map_err(|e| {
        err!(
            InvalidRequest,
            "Invalid cron expression '{}': {}",
            expression,
            e
        )
    })
}

/// 解析 IANA 时区名。
pub fn parse_timezone(timezone: &str) -> Result<Tz> {
    timezone
        .parse::<Tz>()
        .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))
}

/// 标准 5 字段 cron 补 `0 ` 前缀（秒 = 0）；其余原样保留。
fn normalize_expression(expression: &str) -> String {
    let trimmed = expression.trim();
    if trimmed.split_whitespace().count() == 5 {
        format!("0 {}", trimmed)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_5_fields() {
        assert_eq!(normalize_expression("0 4 * * *"), "0 0 4 * * *");
        assert_eq!(normalize_expression("30 8 * * 1-5"), "0 30 8 * * 1-5");
    }

    #[test]
    fn test_normalize_6_fields() {
        assert_eq!(normalize_expression("0 0 4 * * *"), "0 0 4 * * *");
        assert_eq!(normalize_expression("30 8 * * 1-5 *"), "30 8 * * 1-5 *");
    }

    #[test]
    fn test_validate_expression_valid() {
        let now = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(validate_expression("0 4 * * *", "Asia/Shanghai", now).is_ok());
        assert!(validate_expression("0 0 4 * * *", "Asia/Shanghai", now).is_ok());
        assert!(validate_expression("*/15 * * * *", "UTC", now).is_ok());
    }

    #[test]
    fn test_validate_expression_invalid() {
        let now = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(validate_expression("invalid", "Asia/Shanghai", now).is_err());
        assert!(validate_expression("0 25 * * *", "Asia/Shanghai", now).is_err());
    }

    #[test]
    fn test_validate_expression_invalid_timezone() {
        let now = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(validate_expression("0 4 * * *", "Invalid/Zone", now).is_err());
    }

    #[test]
    fn test_next_run_at_daily_4am() {
        // "0 4 * * *" 在 Asia/Shanghai 下 = 每天 04:00 北京时间。
        // 2026-08-14 10:00 UTC = 18:00 北京，下一次是 08-15 04:00 北京 = 08-14 20:00 UTC。
        let from = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at("0 4 * * *", "Asia/Shanghai", from).unwrap();
        assert_eq!(
            DateTime::<Utc>::from_timestamp_millis(next)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-14 20:00"
        );
    }

    #[test]
    fn test_next_run_at_already_past() {
        let from = DateTime::parse_from_rfc3339("2026-08-14T19:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at("0 4 * * *", "Asia/Shanghai", from).unwrap();
        assert_eq!(
            DateTime::<Utc>::from_timestamp_millis(next)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-14 20:00"
        );
    }

    #[test]
    fn test_next_run_at_utc_timezone() {
        let from = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at("0 4 * * *", "UTC", from).unwrap();
        assert_eq!(
            DateTime::<Utc>::from_timestamp_millis(next)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-15 04:00"
        );
    }
}
