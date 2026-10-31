//! 属性域覆盖率纯函数单测（P2：漂移看板 L2 阈值维度）
//!
//! 覆盖六类判定边界：全域内 / 含域外且明细排序与截断 / 空 `domain_classes`
//! （= 不限）/ `term_key` 无属性定义（幽灵属性）/ `total_values = 0` /
//! alert 阈值边界（恰好 0.95 不告警、0.94 告警）。

use super::ontology::{PROPERTY_DOMAIN_COVERAGE_ALERT_THRESHOLD, compute_property_domain_coverage};
use crate::models::ontology::{OntologyProperty, OntologyPropertyPo};
use crate::service::dao::memory::PropertyDomainFreqRow;

/// 构造属性定义（`domain_classes` 传空切片 = 声明为「不限」）
fn definition(term_key: &str, domain_classes: &[&str]) -> OntologyProperty {
    let classes = serde_json::to_string(domain_classes).expect("domain_classes 序列化");
    OntologyProperty::from_po(OntologyPropertyPo::new(
        term_key,
        term_key,
        term_key,
        classes,
        "[\"node\"]",
        "string",
        "",
        "single",
        None,
        false,
    ))
}

/// 构造聚合行
fn row(term_key: &str, node_type: &str, count: i64) -> PropertyDomainFreqRow {
    PropertyDomainFreqRow {
        term_key: term_key.to_string(),
        node_type: node_type.to_string(),
        count,
    }
}

#[test]
fn counts_everything_in_domain_when_all_values_match() {
    let definitions = vec![definition("lifecycle_status", &["pattern", "fact"])];
    let rows = vec![
        row("lifecycle_status", "pattern", 3),
        row("lifecycle_status", "fact", 2),
    ];

    let coverage = compute_property_domain_coverage(&definitions, &rows, 20);

    assert_eq!(coverage.total_values, 5);
    assert_eq!(coverage.in_domain, 5);
    assert_eq!(coverage.out_of_domain, 0);
    assert!((coverage.coverage_ratio - 1.0).abs() < f64::EPSILON);
    assert!(!coverage.alert);
    assert_eq!(coverage.threshold, PROPERTY_DOMAIN_COVERAGE_ALERT_THRESHOLD);
    assert!(coverage.out_of_domain_by_class.is_empty());
}

#[test]
fn reports_out_of_domain_by_class_ordered_and_truncated() {
    let definitions = vec![definition("lifecycle_status", &["pattern"])];
    let rows = vec![
        row("lifecycle_status", "pattern", 5),
        row("lifecycle_status", "concept", 3),
        row("lifecycle_status", "category", 3),
        row("lifecycle_status", "account", 2),
    ];

    let coverage = compute_property_domain_coverage(&definitions, &rows, 2);

    assert_eq!(coverage.total_values, 13);
    assert_eq!(coverage.in_domain, 5);
    assert_eq!(coverage.out_of_domain, 8);
    // count 降序；同 count 按 term_key / node_type 升序（category 先于 concept）；
    // 截断 top_n = 2，只留前两条而 out_of_domain 仍按全量计数
    let pairs: Vec<(&str, &str, u64)> = coverage
        .out_of_domain_by_class
        .iter()
        .map(|item| (item.term_key.as_str(), item.node_type.as_str(), item.count))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("lifecycle_status", "category", 3),
            ("lifecycle_status", "concept", 3),
        ]
    );
}

#[test]
fn empty_domain_classes_means_unrestricted() {
    let definitions = vec![definition("domain", &[])];
    let rows = vec![row("domain", "pattern", 10), row("domain", "account", 5)];

    let coverage = compute_property_domain_coverage(&definitions, &rows, 20);

    // 空清单 = 不限 = 全部计入域内（与 allows_domain_class 同语义）
    assert_eq!(coverage.total_values, 15);
    assert_eq!(coverage.in_domain, 15);
    assert_eq!(coverage.out_of_domain, 0);
    assert!(!coverage.alert);
    assert!(coverage.out_of_domain_by_class.is_empty());
}

#[test]
fn unknown_term_key_counts_as_out_of_domain() {
    let definitions = vec![definition("lifecycle_status", &["pattern"])];
    let rows = vec![
        row("ghost_property", "concept", 4),
        row("lifecycle_status", "pattern", 6),
    ];

    let coverage = compute_property_domain_coverage(&definitions, &rows, 20);

    assert_eq!(coverage.total_values, 10);
    assert_eq!(coverage.in_domain, 6);
    assert_eq!(coverage.out_of_domain, 4);
    assert_eq!(coverage.out_of_domain_by_class.len(), 1);
    let outlier = &coverage.out_of_domain_by_class[0];
    assert_eq!(outlier.term_key, "ghost_property");
    assert_eq!(outlier.node_type, "concept");
    assert_eq!(outlier.count, 4);
    // 6/10 = 0.6 < 0.95
    assert!(coverage.alert);
}

#[test]
fn zero_values_yield_full_coverage_without_alert() {
    let coverage = compute_property_domain_coverage(&[], &[], 20);

    assert_eq!(coverage.total_values, 0);
    assert_eq!(coverage.in_domain, 0);
    assert_eq!(coverage.out_of_domain, 0);
    assert!((coverage.coverage_ratio - 1.0).abs() < f64::EPSILON);
    assert!(!coverage.alert);
    assert!(coverage.out_of_domain_by_class.is_empty());
}

#[test]
fn alert_boundary_is_strictly_below_threshold() {
    let definitions = vec![definition("lifecycle_status", &["pattern"])];

    // 19 域内 / 1 域外 = 0.95，恰好等于阈值 → 不告警
    let at_threshold = vec![
        row("lifecycle_status", "pattern", 19),
        row("lifecycle_status", "concept", 1),
    ];
    let coverage = compute_property_domain_coverage(&definitions, &at_threshold, 20);
    assert!((coverage.coverage_ratio - 0.95).abs() < 1e-9);
    assert!(!coverage.alert);

    // 47 域内 / 3 域外 = 0.94 → 告警
    let below = vec![
        row("lifecycle_status", "pattern", 47),
        row("lifecycle_status", "concept", 3),
    ];
    let coverage = compute_property_domain_coverage(&definitions, &below, 20);
    assert!((coverage.coverage_ratio - 0.94).abs() < 1e-9);
    assert!(coverage.alert);
}
