//! 规则 → 实例的展开算法（纯计算，前后端共用）。

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

use super::cron::parse_schedule;
use crate::enums::{ScheduleDateTerm, TriggerType};
use crate::error::{Result, err};

/// 计算一条触发记录实际生效的时间规则。
///
/// `Schedule` 只是形态标签，时间规则复用 `cron > interval > run_at` 三选一；
/// 调度器推进游标与日程展开必须走同一判定，故下沉到 common 作为单一事实源。
pub fn effective_type(cron_expression: Option<&str>, interval_seconds: Option<i64>) -> TriggerType {
    if cron_expression.is_some() {
        TriggerType::Cron
    } else if interval_seconds.is_some() {
        TriggerType::Interval
    } else {
        TriggerType::Once
    }
}

/// 一次展开所用的规则参数（由调用方从触发器 DTO / 前端表单投影而来）。
#[derive(Debug, Clone, Default)]
pub struct ScheduleRule {
    /// 触发形态：Schedule 是形态标签，实际时间规则看本字段
    pub effective_type: TriggerType,
    /// Cron 表达式（Cron 规则必填）
    pub cron_expression: Option<String>,
    /// 间隔秒数（Interval 规则必填）
    pub interval_seconds: Option<i64>,
    /// 一次性触发时刻（Once 规则）
    pub run_at: Option<i64>,
    /// 当前游标：Interval 的相位锚点 / Once 的兜底时刻
    pub next_run_at: i64,
    /// 规则创建时刻：实例不早于创建时刻
    pub created_at: i64,
    /// 生效窗口起点（None = 无限制）
    pub start_at: Option<i64>,
    /// 生效窗口终点（None = 无限制）
    pub end_at: Option<i64>,
    /// 最大触发次数（None = 不限）
    pub max_runs: Option<i64>,
    /// 已执行次数
    pub total_runs: i64,
}

/// 向上取整除法（`b > 0`）
fn ceil_div(a: i64, b: i64) -> i64 {
    a.div_euclid(b) + i64::from(a.rem_euclid(b) > 0)
}

fn within(rule: &ScheduleRule, ts: i64) -> bool {
    ts >= rule.created_at
        && rule.start_at.is_none_or(|start| ts >= start)
        && rule.end_at.is_none_or(|end| ts <= end)
}

/// 未来实例还能放行几次（`None` = 不限次）。
///
/// `max_runs` 是总次数口径、`total_runs` 是已执行次数，两者之差才是「还能跑几次」。
/// 未来实例的序号**必须相对 `next_run_at` 锚点**计算：以查询下界为 0 起算，
/// 会让整体落在未来的窗口（如「下周」）把剩余次数重复计数，放出永远不会发生的实例。
fn remaining_runs(rule: &ScheduleRule) -> Option<usize> {
    rule.max_runs.map(|max| {
        usize::try_from(max.saturating_sub(rule.total_runs).max(0)).unwrap_or(usize::MAX)
    })
}

/// 单次展开的最大扫描步数。
///
/// 防御性上限：次数受限的规则需要从锚点开始数，若锚点极旧（调度器长期停摆）
/// 且窗口极大（`from=0`），扫描次数会爆炸。到上限即停，宁可少展开也不阻塞调用方。
const MAX_SCAN_STEPS: i64 = 100_000;

/// 展开一条规则在 `[from, to)` 内的所有实例（升序，最多 `max_items` 条）。
///
/// - Interval 以 `next_run_at` 为相位锚点前后双向展开：历史实例回答「昨天干了什么」，
///   未来实例回答「明天干什么」。这是按规则得到的理论实例；实际调度在业务成功后
///   以完成时刻加步长推进（`mark_executed`），因此真实执行点可能相对预览漂移。
/// - Cron 由表达式定义全部候选点，用 `Schedule::after` 从下界逐点取。
/// - Once 只有 `run_at`（存量回落到 `next_run_at`）一个点。
///
/// `from`/`to` 均为 UTC 毫秒。表达式非法或时区非法返回错误；
/// 规则本身字段不全（如 Interval 无步长）返回空列表而不是报错——
/// 存量脏规则不应让整个日程查询失败。
pub fn expand_rule(
    rule: &ScheduleRule,
    from: i64,
    to: i64,
    timezone: &str,
    max_items: usize,
) -> Result<Vec<i64>> {
    if to <= from || max_items == 0 {
        return Ok(Vec::new());
    }
    let lower = from
        .max(rule.created_at)
        .max(rule.start_at.unwrap_or(i64::MIN));
    let upper = to.min(rule.end_at.map(|e| e.saturating_add(1)).unwrap_or(i64::MAX));
    if upper <= lower {
        return Ok(Vec::new());
    }

    match rule.effective_type {
        TriggerType::Once => {
            // 一次性时刻：存量记录可能只写了游标，回落到 `next_run_at`
            let ts = rule.run_at.unwrap_or(rule.next_run_at);
            Ok(if ts >= lower && ts < upper && within(rule, ts) {
                vec![ts]
            } else {
                Vec::new()
            })
        }
        TriggerType::Interval => {
            let Some(seconds) = rule.interval_seconds.filter(|v| *v > 0) else {
                return Ok(Vec::new());
            };
            let step = seconds.saturating_mul(1000);
            if step <= 0 {
                return Ok(Vec::new());
            }
            let anchor = rule.next_run_at;
            let k_start = ceil_div(lower.saturating_sub(anchor), step);
            // 步数上限同时兜住「窗口极大 + 全部被次数过滤」导致的空转
            let k_end = (ceil_div(upper.saturating_sub(anchor), step) - 1)
                .min(k_start.saturating_add(MAX_SCAN_STEPS));
            let remaining = remaining_runs(rule);
            let mut out = Vec::new();
            let mut k = k_start;
            while k <= k_end && out.len() < max_items {
                let ts = anchor.saturating_add(k.saturating_mul(step));
                if ts >= lower && ts < upper && within(rule, ts) {
                    // 历史实例（早于锚点）是既成事实，不受 max_runs 裁剪；
                    // 未来实例的序号 = 相对锚点的步数，只有它在剩余次数内才放行。
                    let allowed = if k < 0 {
                        true
                    } else {
                        remaining.is_none_or(|left| (k as usize) < left)
                    };
                    if allowed {
                        out.push(ts);
                    }
                }
                k += 1;
            }
            Ok(out)
        }
        TriggerType::Cron | TriggerType::Schedule => {
            let Some(expression) = rule.cron_expression.as_deref() else {
                return Ok(Vec::new());
            };
            let tz: Tz = timezone
                .parse()
                .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))?;
            let schedule = parse_schedule(expression)?;
            let remaining = remaining_runs(rule);
            // 有次数上限时必须从锚点开始数：窗口若整体落在未来，从窗口下界起算
            // 会让每个实例的序号都从 0 重来，放大剩余次数。
            let scan_lower = match remaining {
                Some(_) => lower.min(rule.next_run_at),
                None => lower,
            };
            // `after` 是严格大于，故从下界前一毫秒起取，才能包含恰好落在下界的点
            let start = DateTime::<Utc>::from_timestamp_millis(scan_lower.saturating_sub(1))
                .ok_or_else(|| err!(InvalidRequest, "展开下界时间戳越界"))?
                .with_timezone(&tz);
            let mut out = Vec::new();
            let mut future_index = 0usize;
            let mut steps = 0i64;
            for dt in schedule.after(&start) {
                steps += 1;
                if steps > MAX_SCAN_STEPS {
                    break;
                }
                let ts = dt.timestamp_millis();
                if ts >= upper {
                    break;
                }
                if !within(rule, ts) {
                    continue;
                }
                if ts >= rule.next_run_at {
                    let allowed = remaining.is_none_or(|left| future_index < left);
                    if ts >= lower && allowed {
                        out.push(ts);
                    }
                    future_index += 1;
                    if remaining.is_some_and(|left| future_index >= left) {
                        break;
                    }
                } else if ts >= lower {
                    out.push(ts);
                }
                if out.len() >= max_items {
                    break;
                }
            }
            Ok(out)
        }
    }
}

/// 把相对日期词折算为半开区间 `[from, to)`（UTC 毫秒）。
///
/// 所有「哪一天」的判断都按 `timezone` 的本地日历日进行，跨月/跨年/闰年
/// 由 chrono 处理；模型只传日期词，不需要自己算时间戳。
pub fn resolve_term_range(term: ScheduleDateTerm, now: i64, timezone: &str) -> Result<(i64, i64)> {
    let tz: Tz = timezone
        .parse()
        .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))?;
    let local = DateTime::<Utc>::from_timestamp_millis(now)
        .ok_or_else(|| err!(InvalidRequest, "基准时间戳越界"))?
        .with_timezone(&tz);
    let today = local.date_naive();

    let (first, days) = match term {
        ScheduleDateTerm::Yesterday => (
            today
                .pred_opt()
                .ok_or_else(|| err!(InvalidRequest, "日期越界：无法计算昨天"))?,
            1,
        ),
        ScheduleDateTerm::Today => (today, 1),
        ScheduleDateTerm::Tomorrow => (
            today
                .succ_opt()
                .ok_or_else(|| err!(InvalidRequest, "日期越界：无法计算明天"))?,
            1,
        ),
        ScheduleDateTerm::ThreeDays => (
            today
                .pred_opt()
                .ok_or_else(|| err!(InvalidRequest, "日期越界：无法计算昨天"))?,
            3,
        ),
        ScheduleDateTerm::ThisWeek => (monday_of(today), 7),
        ScheduleDateTerm::NextWeek => (monday_of(today) + Duration::days(7), 7),
        ScheduleDateTerm::Custom => {
            return Err(err!(InvalidRequest, "Custom 日期词需要显式提供 from/to"));
        }
    };
    let from = midnight_ms(first, &tz)?;
    let to = midnight_ms(first + Duration::days(days), &tz)?;
    Ok((from, to))
}

/// 取给定日期所在周的周一（ISO 周一起算）。
pub fn monday_of(date: NaiveDate) -> NaiveDate {
    let offset = i64::from(date.weekday().num_days_from_monday());
    date - Duration::days(offset)
}

/// 把 UTC 毫秒折算为指定时区的日历日键 `YYYY-MM-DD`。
///
/// 前端日历按服务端返回值里的 `timezone` 分桶，必须与展开时的日历日一致，
/// 否则浏览器本地时区与 `server.timezone` 不同时实例会落到相邻列。
pub fn calendar_day_key(ts_ms: i64, timezone: &str) -> Result<String> {
    let tz: Tz = timezone
        .parse()
        .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))?;
    let dt = DateTime::<Utc>::from_timestamp_millis(ts_ms)
        .ok_or_else(|| err!(InvalidRequest, "时间戳越界"))?
        .with_timezone(&tz);
    Ok(dt.format("%Y-%m-%d").to_string())
}

/// 从区间起点生成连续 `days` 个日历日键，跨月/跨年由 chrono 处理。
pub fn calendar_day_keys(from_ms: i64, days: usize, timezone: &str) -> Result<Vec<String>> {
    let tz: Tz = timezone
        .parse()
        .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))?;
    let start = DateTime::<Utc>::from_timestamp_millis(from_ms)
        .ok_or_else(|| err!(InvalidRequest, "时间戳越界"))?
        .with_timezone(&tz)
        .date_naive();
    Ok((0..days)
        .map(|i| {
            (start + Duration::days(i as i64))
                .format("%Y-%m-%d")
                .to_string()
        })
        .collect())
}

/// 把 UTC 毫秒格式化为指定时区的 `HH:MM`。
pub fn format_time_hm_in_tz(ts_ms: i64, timezone: &str) -> Result<String> {
    let tz: Tz = timezone
        .parse()
        .map_err(|e| err!(InvalidRequest, "Invalid timezone '{}': {}", timezone, e))?;
    let dt = DateTime::<Utc>::from_timestamp_millis(ts_ms)
        .ok_or_else(|| err!(InvalidRequest, "时间戳越界"))?
        .with_timezone(&tz);
    Ok(dt.format("%H:%M").to_string())
}

/// 某时区下某天的本地零点（UTC 毫秒）。DST 跳变取最早分支保证单调。
fn midnight_ms(date: NaiveDate, tz: &Tz) -> Result<i64> {
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| err!(InvalidRequest, "无法构造本地零点"))?;
    naive
        .and_local_timezone(*tz)
        .earliest()
        .map(|dt| dt.timestamp_millis())
        .ok_or_else(|| err!(InvalidRequest, "本地零点在该时区不存在"))
}

#[cfg(test)]
#[path = "occurrence_tests.rs"]
mod tests;
