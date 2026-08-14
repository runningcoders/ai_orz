//! Cron 表达式解析工具
//!
//! 基于 `cron` crate + `chrono-tz` 实现定时任务的 cron 表达式解析。
//! 支持标准 5 字段（分 时 日 月 周）和扩展 6-7 字段格式。
//!
//! 时区通过系统配置 `server.timezone` 获取（默认 "Asia/Shanghai"）。

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use common::error::{Result, err};

/// 解析 cron 表达式并计算从指定时间起的下一次触发时间（UTC 时间戳，秒）
///
/// # 参数
/// - `expression`: cron 表达式（5 字段或 6-7 字段）
/// - `timezone`: IANA 时区名（如 "Asia/Shanghai"）
/// - `from`: 从哪个时间点开始计算（UTC）
///
/// # 返回
/// 下一次触发时间的 UTC 时间戳（秒），与 `next_run_at` 字段单位一致
pub fn next_run_at(expression: &str, timezone: &str, from: DateTime<Utc>) -> Result<i64> {
    let tz = parse_timezone(timezone)?;
    let schedule = parse_schedule(expression)?;
    let from_tz = from.with_timezone(&tz);
    let next = schedule
        .after(&from_tz)
        .next()
        .ok_or_else(|| {
            err!(
                InvalidRequest,
                "No future trigger time for cron expression '{}'",
                expression
            )
        })?;
    Ok(next.with_timezone(&Utc).timestamp())
}

/// 校验 cron 表达式是否合法（能在指定时区下产生未来的触发时间）
pub fn validate_expression(expression: &str, timezone: &str) -> Result<()> {
    let tz = parse_timezone(timezone)?;
    let schedule = parse_schedule(expression)?;
    let now_tz = Utc::now().with_timezone(&tz);
    schedule.after(&now_tz).next().ok_or_else(|| {
        err!(
            InvalidRequest,
            "No future trigger time for cron expression '{}'",
            expression
        )
    })?;
    Ok(())
}

/// 获取系统配置的时区
pub fn system_timezone() -> String {
    crate::config::get().server.timezone.clone()
}

/// 解析时区字符串
fn parse_timezone(timezone: &str) -> Result<Tz> {
    timezone.parse::<Tz>().map_err(|e| {
        err!(
            InvalidRequest,
            "Invalid timezone '{}': {}",
            timezone,
            e
        )
    })
}

/// 解析 cron 表达式为 Schedule
///
/// cron crate 要求 6-7 字段（含秒），标准 Unix cron 是 5 字段（不含秒）。
/// 如果检测到 5 字段，自动补 "0 " 前缀（秒 = 0）。
fn parse_schedule(expression: &str) -> Result<cron::Schedule> {
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

/// 规范化 cron 表达式：5 字段补 "0 " 前缀（秒字段）
fn normalize_expression(expression: &str) -> String {
    let trimmed = expression.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
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
        assert!(validate_expression("0 4 * * *", "Asia/Shanghai").is_ok());
        assert!(validate_expression("0 0 4 * * *", "Asia/Shanghai").is_ok());
        assert!(validate_expression("*/15 * * * *", "UTC").is_ok());
    }

    #[test]
    fn test_validate_expression_invalid() {
        assert!(validate_expression("invalid", "Asia/Shanghai").is_err());
        assert!(validate_expression("0 25 * * *", "Asia/Shanghai").is_err());
    }

    #[test]
    fn test_validate_expression_invalid_timezone() {
        assert!(validate_expression("0 4 * * *", "Invalid/Zone").is_err());
    }

    #[test]
    fn test_next_run_at_daily_4am() {
        // 构造一个 2026-08-14 10:00 UTC 的时间点
        let from = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // "0 4 * * *" 在 Asia/Shanghai 时区下 = 每天凌晨 4 点（北京时间）
        // 2026-08-14 10:00 UTC = 2026-08-14 18:00 北京时间
        // 下一个凌晨 4 点 = 2026-08-15 04:00 北京时间 = 2026-08-14 20:00 UTC
        let next = next_run_at("0 4 * * *", "Asia/Shanghai", from).unwrap();
        let next_dt = DateTime::<Utc>::from_timestamp(next, 0).unwrap();
        assert_eq!(next_dt.format("%Y-%m-%d %H:%M").to_string(), "2026-08-14 20:00");
    }

    #[test]
    fn test_next_run_at_already_past() {
        // 2026-08-14 19:00 UTC = 2026-08-15 03:00 北京时间
        // 下一个凌晨 4 点 = 2026-08-15 04:00 北京时间 = 2026-08-14 20:00 UTC
        let from = DateTime::parse_from_rfc3339("2026-08-14T19:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at("0 4 * * *", "Asia/Shanghai", from).unwrap();
        let next_dt = DateTime::<Utc>::from_timestamp(next, 0).unwrap();
        assert_eq!(next_dt.format("%Y-%m-%d %H:%M").to_string(), "2026-08-14 20:00");
    }

    #[test]
    fn test_next_run_at_utc_timezone() {
        // UTC 时区下 "0 4 * * *" = 每天凌晨 4 点 UTC
        let from = DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_at("0 4 * * *", "UTC", from).unwrap();
        let next_dt = DateTime::<Utc>::from_timestamp(next, 0).unwrap();
        assert_eq!(next_dt.format("%Y-%m-%d %H:%M").to_string(), "2026-08-15 04:00");
    }
}
