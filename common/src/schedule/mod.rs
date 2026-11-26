//! 前后端共用的时间算法。
//!
//! 统一调度器只持久化「规则 + 一个游标 `next_run_at`」，不存实例表。
//! 查询「明天 / 本周」或前端在日历上预览待创建的日程时，都需要把规则在
//! 任意区间上展开成一条条实例。本模块只做纯计算，不读配置、不碰数据库：
//! 输入规则与区间，输出落在区间内的发生时刻（Unix 毫秒）。
//!
//! 放在 `common` 而非服务端 `pkg`，是为了让前端在日历上直接增加日程时，
//! 不必先落库再往返服务端即可预览展开结果。

mod cron;
mod occurrence;

use crate::error::{Result, bail_err};

pub use cron::{next_run_at, parse_schedule, validate_expression};
pub use occurrence::{
    ScheduleRule, calendar_day_key, calendar_day_keys, effective_type, expand_rule,
    format_time_hm_in_tz, monday_of, resolve_term_range,
};

/// 由 interval 规则推算下一次触发时间（Unix 毫秒）。
///
/// 两个边界在这里一次收口，避免各入口各写一遍 `now + interval * 1000`：
/// - `interval_seconds` 必须 > 0，否则等价于「创建即到点、反复触发」；
/// - 乘法与加法都走 `checked_*`：极端值（`i64::MAX`）必须报错，
///   而不是 wrap 成过去时间被调度器立刻领取。
pub fn interval_next_run_at(now_ms: i64, interval_seconds: i64) -> Result<i64> {
    if interval_seconds <= 0 {
        bail_err!(InvalidRequest, "interval_seconds 必须大于 0");
    }
    let delta = interval_seconds.checked_mul(1000).ok_or_else(|| {
        crate::error::err!(
            InvalidRequest,
            "interval_seconds 过大：{} 秒超出可表示的毫秒范围",
            interval_seconds
        )
    })?;
    now_ms.checked_add(delta).ok_or_else(|| {
        crate::error::err!(InvalidRequest, "interval_seconds 过大：下一次触发时间越界")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_adds_millis() {
        assert_eq!(interval_next_run_at(1_000, 60).unwrap(), 61_000);
    }

    #[test]
    fn interval_rejects_non_positive() {
        assert!(interval_next_run_at(1_000, 0).is_err());
        assert!(interval_next_run_at(1_000, -5).is_err());
    }

    #[test]
    fn interval_rejects_overflow_instead_of_wrapping() {
        assert!(interval_next_run_at(0, i64::MAX).is_err());
        assert!(interval_next_run_at(i64::MAX - 1, 1).is_err());
    }
}
