//! 决策契约的语义护栏
//!
//! 这里只测「缺失 ≠ 否」与「权重即排序依据」两件事——它们是下游
//! （策略判定、检索精选）最容易静默出错的地方。

use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn get_as_round_trips_scene_defined_structs() {
    // 场景自定义的复合输入走同一个事实桶，解析失败按「没有这个事实」处理
    let m = Metrics::new()
        .with("candidates", vec!["a", "b"])
        .with("question", "q");

    assert_eq!(
        m.get_as::<Vec<String>>("candidates"),
        Some(vec!["a".to_string(), "b".to_string()])
    );
    // 类型不符 → None，不 panic
    assert!(m.get_as::<Vec<u64>>("candidates").is_none());
    assert!(m.get_as::<String>("missing").is_none());
}

#[test]
fn metrics_without_decisions_reports_no_basis() {
    let m = Metrics::new();
    assert!(!m.has_decisions());
    // None = 没有判断依据，调用方不得压成「否」
    assert!(m.decision("any").is_none());
}

#[test]
fn missing_answer_stays_none() {
    // 答案里只有 a：问 b 必须拿到 None，而不是某个默认值
    let answers = DecisionAnswers::new().with("a", DecisionValue::Presence { value: 0.25 });
    assert_eq!(answers.len(), 1);
    assert!(answers.get("b").is_none());

    let m = Metrics::new().with_decisions(Arc::new(answers));
    assert!(m.has_decisions());
    assert!(matches!(
        m.decision("a"),
        Some(DecisionValue::Presence { .. })
    ));
    assert!(m.decision("b").is_none());
}

#[test]
fn pick_keeps_all_option_weights_for_ranking() {
    // 一次 Pick 就拿到全序：weights 是「候选排序」的唯一依据
    let weights = BTreeMap::from([
        ("m1".to_string(), 0.7_f32),
        ("m2".to_string(), 0.2_f32),
        ("m3".to_string(), 0.1_f32),
    ]);
    let answers = DecisionAnswers::new().with(
        "rank",
        DecisionValue::Pick {
            chosen: "m1".to_string(),
            confidence: 0.7,
            weights: weights.clone(),
        },
    );

    match answers.get("rank") {
        Some(DecisionValue::Pick {
            chosen,
            confidence,
            weights: got,
        }) => {
            assert_eq!(chosen, "m1");
            assert!((confidence - 0.7).abs() < f32::EPSILON);
            assert_eq!(got, &weights);
        }
        other => panic!("expected Pick, got {other:?}"),
    }
}
