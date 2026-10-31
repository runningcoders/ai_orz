//! 属性有效性统计纯函数单测（P3：属性 × 实体类 众数占比 → >90% 标黄）
//!
//! 覆盖八类判定边界：全单值组标黄 / 真实分布不标黄（非众数占比）/
//! 并列众数取字典序最小 / flagged 阈值严格大于（恰好 0.90 不标、0.9001 标）/
//! 过滤（term_key、node_type，含归一化）/ limit 截断与 total_groups / 空集不产条目 /
//! 排序确定性（占比降序 → term_key 升序 → node_type 升序）。

use super::ontology::{
    PROPERTY_EFFECTIVENESS_MODE_ALERT_THRESHOLD, compute_property_effectiveness,
};
use crate::service::dao::memory::PropertyValueFreqRow;
use common::api::ontology::GetPropertyEffectivenessRequest;

/// 构造聚合行（`(term_key, node_type, 取值)` 的生效条数）
fn row(term_key: &str, node_type: &str, value: &str, count: i64) -> PropertyValueFreqRow {
    PropertyValueFreqRow {
        term_key: term_key.to_string(),
        node_type: node_type.to_string(),
        value_text: value.to_string(),
        count,
    }
}

/// 构造请求（未显式给出的字段 = 默认语义：不过滤、不筛占比、limit 走默认 100）
fn request(
    term_key: Option<&str>,
    node_type: Option<&str>,
    min_mode_ratio: Option<f64>,
    limit: Option<usize>,
) -> GetPropertyEffectivenessRequest {
    GetPropertyEffectivenessRequest {
        term_key: term_key.map(str::to_string),
        node_type: node_type.map(str::to_string),
        min_mode_ratio,
        limit,
    }
}

/// 浮点近似比较（单测里避免直接 `==` 比较 f64）
fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "期望 {}，实际 {}",
        expected,
        actual
    );
}

#[test]
fn single_value_group_is_flagged_as_constant() {
    // 现网 pattern 基线形态：87 条全「现行有效」→ 众数占比 1.0 → 标黄
    let rows = vec![row("lifecycle_status", "pattern", "现行有效", 87)];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));

    assert_close(resp.threshold, PROPERTY_EFFECTIVENESS_MODE_ALERT_THRESHOLD);
    assert_eq!(resp.total_groups, 1);
    assert_eq!(resp.items.len(), 1);

    let item = &resp.items[0];
    assert_eq!(item.term_key, "lifecycle_status");
    assert_eq!(item.node_type, "pattern");
    assert_eq!(item.total, 87);
    assert_eq!(item.distinct_values, 1);
    assert_eq!(item.mode_value, "现行有效");
    assert_eq!(item.mode_count, 87);
    assert_close(item.mode_ratio, 1.0);
    assert_close(item.non_default_ratio, 0.0);
    assert!(item.flagged, "占比 1.0 必须标黄");
}

#[test]
fn real_distribution_keeps_group_unflagged() {
    // 现网 fact 基线形态：多值共存 → 众数占比明显 < 0.90 → 不标黄
    let rows = vec![
        row("lifecycle_status", "fact", "现行有效", 60),
        row("lifecycle_status", "fact", "进行中", 7),
        row("lifecycle_status", "fact", "已完成", 1),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));
    let item = &resp.items[0];

    assert_eq!(item.total, 68);
    assert_eq!(item.distinct_values, 3);
    assert_eq!(item.mode_value, "现行有效");
    assert_eq!(item.mode_count, 60);
    assert_close(item.mode_ratio, 60.0 / 68.0);
    assert_close(item.non_default_ratio, 8.0 / 68.0);
    assert!(!item.flagged, "占比 {:.4} < 0.90 不应标黄", item.mode_ratio);
}

#[test]
fn mode_tie_broken_by_lexicographically_smallest_value() {
    // 并列众数（count 相同）取字典序最小：alpha 而非 beta，且与插入顺序无关
    let rows = vec![
        row("lifecycle_status", "concept", "beta", 3),
        row("lifecycle_status", "concept", "alpha", 3),
        row("lifecycle_status", "concept", "gamma", 1),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));
    let item = &resp.items[0];

    assert_eq!(item.distinct_values, 3);
    assert_eq!(item.total, 7);
    assert_eq!(item.mode_value, "alpha", "平局必须取字典序最小，保证确定性");
    assert_eq!(item.mode_count, 3);
    assert_close(item.mode_ratio, 3.0 / 7.0);

    // 换一次输入顺序，结论必须一致（这就是「可回归」的意义）
    let reordered = vec![
        row("lifecycle_status", "concept", "alpha", 3),
        row("lifecycle_status", "concept", "gamma", 1),
        row("lifecycle_status", "concept", "beta", 3),
    ];
    let resp2 = compute_property_effectiveness(&reordered, &request(None, None, None, None));
    assert_eq!(resp2.items[0].mode_value, "alpha");
}

#[test]
fn flagged_boundary_is_strictly_greater_than_threshold() {
    // 恰好 0.90 → 不标；0.9001 → 标（严格大于）
    let rows = vec![
        row("at_threshold", "fact", "yes", 90),
        row("at_threshold", "fact", "no", 10),
        row("above_threshold", "fact", "yes", 9001),
        row("above_threshold", "fact", "no", 999),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));

    let at = resp
        .items
        .iter()
        .find(|i| i.term_key == "at_threshold")
        .expect("at_threshold 分组");
    assert_close(at.mode_ratio, 0.90);
    assert!(!at.flagged, "mode_ratio 恰好 0.90 不应标黄（严格大于）");

    let above = resp
        .items
        .iter()
        .find(|i| i.term_key == "above_threshold")
        .expect("above_threshold 分组");
    assert_close(above.mode_ratio, 0.9001);
    assert!(above.flagged, "mode_ratio 0.9001 必须标黄");
}

#[test]
fn filters_by_term_key_and_node_type_after_normalization() {
    let rows = vec![
        row("lifecycle_status", "fact", "现行有效", 3),
        row("lifecycle_status", "pattern", "现行有效", 2),
        row("event_time", "fact", "2026-01-01", 1),
    ];

    // term_key 过滤：入口归一化（大小写 / 空白）后与落库规范形比对
    let by_term = compute_property_effectiveness(
        &rows,
        &request(Some(" Lifecycle_Status "), None, None, None),
    );
    assert_eq!(by_term.total_groups, 2);
    assert!(
        by_term
            .items
            .iter()
            .all(|i| i.term_key == "lifecycle_status")
    );

    // node_type 过滤同理（大小写归一）
    let by_class = compute_property_effectiveness(&rows, &request(None, Some("FACT"), None, None));
    assert_eq!(by_class.total_groups, 2);
    assert!(by_class.items.iter().all(|i| i.node_type == "fact"));

    // 双过滤取交集
    let both = compute_property_effectiveness(
        &rows,
        &request(Some("lifecycle_status"), Some("pattern"), None, None),
    );
    assert_eq!(both.total_groups, 1);
    assert_eq!(both.items[0].total, 2);

    // 空串 / 纯空白 = 不过滤（与 DAO 的 None 同义）
    let blank = compute_property_effectiveness(&rows, &request(Some("   "), Some(""), None, None));
    assert_eq!(blank.total_groups, 3);
}

#[test]
fn respects_limit_and_reports_total_groups_before_truncation() {
    // 三组占比各不相同：1.0 > 0.75 > 0.5，排序后 limit 截断
    let rows = vec![
        row("aaa", "fact", "x", 2),
        row("aaa", "fact", "y", 2),
        row("mmm", "fact", "x", 3),
        row("mmm", "fact", "y", 1),
        row("zzz", "fact", "x", 1),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, Some(2)));

    assert_eq!(resp.total_groups, 3, "total_groups 是截断前的分组数");
    assert_eq!(resp.items.len(), 2, "limit 截断生效");
    assert_eq!(resp.items[0].term_key, "zzz");
    assert_close(resp.items[0].mode_ratio, 1.0);
    assert_eq!(resp.items[1].term_key, "mmm");
    assert_close(resp.items[1].mode_ratio, 0.75);

    // limit = 0 是合法入参（只要 total_groups，不要明细）
    let none_shown = compute_property_effectiveness(&rows, &request(None, None, None, Some(0)));
    assert_eq!(none_shown.total_groups, 3);
    assert!(none_shown.items.is_empty());
}

#[test]
fn sorts_by_ratio_then_term_key_then_node_type() {
    // 占比相同的两组：先比 term_key 升序，再比 node_type 升序（结果可回归）
    let rows = vec![
        row("bbb", "pattern", "x", 1),
        row("bbb", "fact", "x", 1),
        row("aaa", "concept", "y", 2),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));
    let keys: Vec<(&str, &str)> = resp
        .items
        .iter()
        .map(|i| (i.term_key.as_str(), i.node_type.as_str()))
        .collect();

    assert_eq!(
        keys,
        vec![("aaa", "concept"), ("bbb", "fact"), ("bbb", "pattern")]
    );
}

#[test]
fn min_mode_ratio_filter_and_empty_input() {
    let rows = vec![
        row("constant_prop", "fact", "x", 10),
        row("mixed_prop", "fact", "x", 6),
        row("mixed_prop", "fact", "y", 4),
    ];

    let filtered = compute_property_effectiveness(&rows, &request(None, None, Some(0.9), None));
    assert_eq!(filtered.total_groups, 1, "只留占比 >= 0.9 的分组");
    assert_eq!(filtered.items[0].term_key, "constant_prop");

    // 边界：>= 而非 >（0.6 的分组在 min=0.6 时应保留）
    let inclusive = compute_property_effectiveness(&rows, &request(None, None, Some(0.6), None));
    assert_eq!(inclusive.total_groups, 2);

    // NaN 视为「不筛」（脏参数不清空结果）
    let nan_filter =
        compute_property_effectiveness(&rows, &request(None, None, Some(f64::NAN), None));
    assert_eq!(nan_filter.total_groups, 2);

    // 空集：不产生条目，但阈值照常回显
    let empty = compute_property_effectiveness(&[], &request(None, None, None, None));
    assert!(empty.items.is_empty());
    assert_eq!(empty.total_groups, 0);
    assert_close(empty.threshold, PROPERTY_EFFECTIVENESS_MODE_ALERT_THRESHOLD);
}

#[test]
fn zero_or_negative_count_rows_are_ignored() {
    // COUNT(*) 恒非负，这里锁死脏数据防御口径：total = 0 的组不产出条目
    let rows = vec![
        row("ghost", "fact", "x", 0),
        row("mixed", "fact", "x", -3),
        row("mixed", "fact", "y", 2),
    ];
    let resp = compute_property_effectiveness(&rows, &request(None, None, None, None));

    assert_eq!(resp.total_groups, 1);
    assert_eq!(resp.items[0].term_key, "mixed");
    assert_eq!(resp.items[0].total, 2);
    assert_eq!(resp.items[0].mode_value, "y");
    assert_close(resp.items[0].mode_ratio, 1.0);
}
