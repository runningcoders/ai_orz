//! `expand_rule` / 日期区间解析的单元测试。
//!
//! 单独成文件而非内联 `mod tests`：用例已接近内联测试体量门禁（200 行），
//! 再补一个就会挡 `make ci`。

use super::*;

fn rule(kind: TriggerType) -> ScheduleRule {
    ScheduleRule {
        effective_type: kind,
        created_at: 0,
        ..Default::default()
    }
}

#[test]
fn effective_type_priority() {
    assert_eq!(
        effective_type(Some("0 9 * * *"), Some(60)),
        TriggerType::Cron
    );
    assert_eq!(effective_type(None, Some(60)), TriggerType::Interval);
    assert_eq!(effective_type(None, None), TriggerType::Once);
}

#[test]
fn ceil_div_rounds_up() {
    assert_eq!(ceil_div(10, 3), 4);
    assert_eq!(ceil_div(9, 3), 3);
    assert_eq!(ceil_div(-1, 3), 0);
    assert_eq!(ceil_div(-4, 3), -1);
}

#[test]
fn interval_expands_around_cursor() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(3600);
    r.next_run_at = 10_000_000;
    let got = expand_rule(&r, 0, 10_000_000 + 3 * 3_600_000, "UTC", 20).unwrap();
    // 锚点前后都要展开：历史实例回答「昨天干了什么」，未来实例回答「明天干什么」
    assert_eq!(
        got,
        vec![
            2_800_000,
            6_400_000,
            10_000_000,
            10_000_000 + 3_600_000,
            10_000_000 + 7_200_000
        ]
    );
}

#[test]
fn interval_respects_created_at_and_window() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(60);
    r.next_run_at = 100_000;
    r.created_at = 90_000;
    r.start_at = Some(95_000);
    r.end_at = Some(130_000);
    let got = expand_rule(&r, 0, 200_000, "UTC", 50).unwrap();
    assert_eq!(got.first(), Some(&100_000));
    assert!(got.iter().all(|ts| *ts >= 95_000 && *ts <= 130_000));
}

#[test]
fn interval_future_limited_by_remaining_runs() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(60);
    r.next_run_at = 100_000;
    r.total_runs = 2;
    r.max_runs = Some(3);
    let got = expand_rule(&r, 0, 400_000, "UTC", 50).unwrap();
    // 40_000 是锚点前的历史实例，不受 max_runs 裁剪；
    // total_runs=2 / max=3 只剩 1 次，故只展示 100_000，不能多放 160_000。
    assert_eq!(got, vec![40_000, 100_000]);
}

#[test]
fn interval_exhausted_hides_future_instances() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(60);
    r.next_run_at = 100_000;
    r.total_runs = 3;
    r.max_runs = Some(3);
    let got = expand_rule(&r, 0, 400_000, "UTC", 50).unwrap();
    // 次数已耗尽：历史仍如实展示，未来一次都不放行
    assert_eq!(got, vec![40_000]);
}

#[test]
fn interval_run_index_is_relative_to_anchor() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(60);
    r.next_run_at = 100_000;
    r.total_runs = 2;
    r.max_runs = Some(3); // 只剩 1 次
    // 窗口整体落在锚点之后很远：序号若从窗口下界重新起算，会放出永远不会发生的实例
    let got = expand_rule(&r, 400_000, 800_000, "UTC", 50).unwrap();
    assert!(
        got.is_empty(),
        "剩余次数已耗尽，未来窗口不应有实例: {got:?}"
    );
    // 窗口覆盖锚点：「只剩 1 次」意味着只放行锚点这一次
    let got = expand_rule(&r, 100_000, 800_000, "UTC", 50).unwrap();
    assert_eq!(got, vec![100_000]);
}

#[test]
fn cron_run_index_is_relative_to_anchor() {
    let mut r = rule(TriggerType::Cron);
    r.cron_expression = Some("0 9 * * *".to_string());
    // 2026-08-14 00:00 UTC
    let from = 1_785_667_200_000i64;
    r.next_run_at = from + 9 * 3_600_000;
    r.total_runs = 2;
    r.max_runs = Some(3); // 只剩 1 次
    // 下周窗口：剩余次数已用尽，一条都不该出现（否则日历会画出 7 条幻影日程）
    let next_week = from + 7 * 86_400_000;
    let got = expand_rule(&r, next_week, next_week + 86_400_000, "UTC", 50).unwrap();
    assert!(
        got.is_empty(),
        "剩余次数已耗尽，下周窗口不应有实例: {got:?}"
    );
    // 本周窗口（3 天）：只剩锚点那一次，不能因为窗口里有 3 天就放出 3 条
    let got = expand_rule(&r, from, from + 3 * 86_400_000, "UTC", 50).unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0] >= from);
}

#[test]
fn huge_interval_window_does_not_spin() {
    let mut r = rule(TriggerType::Interval);
    r.interval_seconds = Some(1);
    r.next_run_at = 0;
    r.total_runs = 1;
    r.max_runs = Some(1); // 一次都不放行：避免「输出为空 → 循环跑到窗口尽头」
    let got = expand_rule(&r, 0, i64::MAX / 2, "UTC", 10).unwrap();
    assert!(got.is_empty());
}

#[test]
fn cron_daily_expands_one_per_day() {
    let mut r = rule(TriggerType::Cron);
    r.cron_expression = Some("0 9 * * *".to_string());
    r.next_run_at = 0;
    // 2026-08-14 00:00 UTC 起两天，Asia/Shanghai 每天 09:00 = UTC 01:00
    let from = 1_785_667_200_000i64;
    let got = expand_rule(&r, from, from + 172_800_000, "Asia/Shanghai", 20).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(
        DateTime::<Utc>::from_timestamp_millis(got[0])
            .unwrap()
            .format("%H:%M")
            .to_string(),
        "01:00"
    );
}

#[test]
fn once_only_its_own_point() {
    let mut r = rule(TriggerType::Once);
    r.run_at = Some(150_000);
    r.next_run_at = 150_000;
    assert_eq!(
        expand_rule(&r, 0, 200_000, "UTC", 10).unwrap(),
        vec![150_000]
    );
    assert!(
        expand_rule(&r, 200_000, 300_000, "UTC", 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn term_range_uses_local_midnight() {
    // 2026-08-14 10:00 UTC = 北京时间 18:00，今天区间应是北京时间 00:00~24:00
    let now = 1_785_705_600_000i64;
    let (from, to) = resolve_term_range(ScheduleDateTerm::Today, now, "Asia/Shanghai").unwrap();
    assert_eq!(to - from, 86_400_000);
    assert_eq!(
        DateTime::<Utc>::from_timestamp_millis(from)
            .unwrap()
            .format("%H:%M")
            .to_string(),
        "16:00"
    );
}

#[test]
fn week_starts_on_monday() {
    let now = 1_785_705_600_000i64;
    let (from, _) = resolve_term_range(ScheduleDateTerm::ThisWeek, now, "Asia/Shanghai").unwrap();
    let local = DateTime::<Utc>::from_timestamp_millis(from)
        .unwrap()
        .with_timezone(&"Asia/Shanghai".parse::<Tz>().unwrap());
    assert_eq!(local.weekday().num_days_from_monday(), 0);
    assert_eq!(local.format("%H:%M").to_string(), "00:00");
}
