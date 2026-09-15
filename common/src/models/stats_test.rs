//! 时序粒度护栏（`STATS_MAX_BUCKETS` / `clamp_interval_to_span`）的契约测试。
//!
//! 关注三件事：
//! 1. 桶数换算本身正确（含窗口缺失时记 0）；
//! 2. 收敛只沿「细 → 粗」方向走，不会把粒度意外变细；
//! 3. 边界值按「≤ 上限即放行」处理，不出现差一错误。

use crate::models::{STATS_MAX_BUCKETS, StatsInterval, clamp_interval_to_span};

const MIN_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

#[test]
fn bucket_ms_matches_declared_tier() {
    assert_eq!(StatsInterval::Minutely.bucket_ms(), MIN_MS);
    assert_eq!(StatsInterval::Hourly.bucket_ms(), HOUR_MS);
    assert_eq!(StatsInterval::Daily.bucket_ms(), DAY_MS);
}

#[test]
fn bucket_count_is_zero_for_missing_or_invalid_span() {
    for interval in [
        StatsInterval::Minutely,
        StatsInterval::Hourly,
        StatsInterval::Daily,
    ] {
        assert_eq!(interval.bucket_count(0), 0);
        assert_eq!(interval.bucket_count(-1), 0);
    }
}

#[test]
fn bucket_count_divides_span_by_bucket_width() {
    assert_eq!(StatsInterval::Minutely.bucket_count(60 * MIN_MS), 60);
    assert_eq!(StatsInterval::Hourly.bucket_count(2 * DAY_MS), 48);
    assert_eq!(StatsInterval::Daily.bucket_count(30 * DAY_MS), 30);
}

#[test]
fn clamp_keeps_interval_when_span_is_unknown() {
    // 窗口缺失/非法时不干预，交回调用方的默认窗口兜底
    for span in [0, -1, i64::MIN] {
        assert_eq!(
            clamp_interval_to_span(StatsInterval::Minutely, span),
            StatsInterval::Minutely
        );
    }
}

#[test]
fn clamp_keeps_minutely_within_budget() {
    // 侧栏「最近 60 分钟」与详情页「最近 1 小时」：60 桶，远低于上限
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, 60 * MIN_MS),
        StatsInterval::Minutely
    );
    // 详情页三档切分的上界：3 小时 = 180 桶
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, 3 * HOUR_MS),
        StatsInterval::Minutely
    );
}

#[test]
fn clamp_boundary_treats_exactly_max_buckets_as_allowed() {
    let exactly = STATS_MAX_BUCKETS * MIN_MS;
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, exactly),
        StatsInterval::Minutely
    );
    // 多一个桶就越界，回退到小时桶
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, exactly + MIN_MS),
        StatsInterval::Hourly
    );
}

#[test]
fn clamp_degrades_minutely_over_wide_window() {
    // 30 天窗口配分钟桶：43200 桶 → 直落天桶（小时桶 720 桶仍超限）
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, 30 * DAY_MS),
        StatsInterval::Daily
    );
    // 4 天窗口配分钟桶：5760 桶超限，小时桶 96 桶合规 → 停在小时桶
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Minutely, 4 * DAY_MS),
        StatsInterval::Hourly
    );
}

#[test]
fn clamp_degrades_hourly_only_when_needed() {
    // 详情页「最近 1 天」「最近 7 天」都在小时内放行
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Hourly, DAY_MS),
        StatsInterval::Hourly
    );
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Hourly, 7 * DAY_MS),
        StatsInterval::Hourly
    );
    // 30 天窗口配小时桶：720 桶超限 → 天桶
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Hourly, 30 * DAY_MS),
        StatsInterval::Daily
    );
}

#[test]
fn clamp_never_returns_a_finer_tier_than_requested() {
    // 收敛是单向的：天桶配小窗口也只保持天桶，不会被「优化」成小时桶
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Daily, DAY_MS),
        StatsInterval::Daily
    );
    assert_eq!(
        clamp_interval_to_span(StatsInterval::Hourly, 30 * MIN_MS),
        StatsInterval::Hourly
    );
}
