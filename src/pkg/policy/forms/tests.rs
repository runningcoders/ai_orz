//! 形态类的行为护栏
//!
//! 每个形态只测三件事：请求形状对不对、结论读取的阈值/缺失语义、降级时给 `None`。

use super::*;
use crate::pkg::policy::{
    DecisionAnswers, DecisionPolicy, DecisionValue, Metrics, Policy, QuestionSpec,
};
use std::collections::BTreeMap;
use std::sync::Arc;

fn rank() -> RankPolicy {
    RankPolicy::new(
        "test_rank",
        "排序",
        "候选 ≥ 2 条时重排",
        "按相关性排序",
        0.08,
        3,
    )
}

fn cands(items: &[(&str, &str)]) -> Vec<Candidate> {
    items
        .iter()
        .map(|(id, text)| Candidate::new(*id, *text))
        .collect()
}

fn pick_metrics(weights: &[(&str, f32)], chosen: &str, policy: &RankPolicy) -> Metrics {
    let answers = DecisionAnswers::new().with(
        policy.primary_qid(),
        DecisionValue::Pick {
            chosen: chosen.to_string(),
            confidence: 0.9,
            weights: weights
                .iter()
                .map(|(id, w)| ((*id).to_string(), *w))
                .collect(),
        },
    );
    Metrics::new().with_decisions(Arc::new(answers))
}

// ==================== 排序 ====================

#[test]
fn rank_builds_one_question_with_all_candidates() {
    let m = Metrics::new().with(KEY_QUESTION, "订单怎么流转").with(
        KEY_CANDIDATES,
        cands(&[("m1", "订单状态机"), ("m2", "退款流程")]),
    );
    let policy = rank();

    let reqs = policy.prepare(&m);
    assert_eq!(reqs.len(), 1, "一次请求问完，不是每条候选各问一次");
    let qid = policy.primary_qid();
    match &reqs[0].questions[&qid] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options.len(), 2);
            assert!(options["m1"].contains("订单状态机"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
    assert_eq!(reqs[0].state["candidate_count"], 2);
}

#[test]
fn rank_skips_when_fewer_than_two_usable_candidates() {
    // 空判据不占选项位：两个候选去掉一个空文本后只剩一个 → 不发起决策
    let m = Metrics::new()
        .with(KEY_QUESTION, "q")
        .with(KEY_CANDIDATES, cands(&[("m1", ""), ("m2", "b")]));
    assert!(rank().prepare(&m).is_empty());
}

#[test]
fn rank_keeps_by_threshold_and_limit() {
    let policy = rank();
    // `ranked` 要候选集做合法性过滤与同分兜底，所以上下文必须一起带上
    let m = pick_metrics(
        &[("m1", 0.02), ("m2", 0.5), ("m3", 0.3), ("m4", 0.18)],
        "m2",
        &policy,
    )
    .with(
        KEY_CANDIDATES,
        cands(&[("m1", "a"), ("m2", "b"), ("m3", "c"), ("m4", "d")]),
    );
    // 阈值 0.08 剔掉 m1；上限 3 截断
    assert_eq!(
        policy.ranked(&m),
        Some(vec!["m2".to_string(), "m3".to_string(), "m4".to_string()])
    );
}

#[test]
fn rank_without_answer_is_not_triggered() {
    let m = Metrics::new().with(KEY_CANDIDATES, cands(&[("m1", "a"), ("m2", "b")]));
    let policy = rank();
    assert!(!policy.is_triggered(&m));
    assert!(policy.ranked(&m).is_none());
}

#[test]
fn rank_ignores_fabricated_ids() {
    let policy = rank();
    let m = pick_metrics(&[("m1", 0.5), ("ghost", 0.9)], "m1", &policy);
    // 排序取保留集要靠候选集过滤，所以这里显式带上下文
    let m = m.with(KEY_CANDIDATES, cands(&[("m1", "a"), ("m2", "b")]));
    assert_eq!(policy.ranked(&m), Some(vec!["m1".to_string()]));
}

// ==================== 单选 ====================

fn route() -> PickOnePolicy {
    PickOnePolicy::new(
        "test_route",
        "路由",
        "判断是否直回",
        "选择一种处理方式",
        [("trivial", "琐碎，可直回"), ("needed", "需要完整思考")],
        0.85,
    )
    .with_state_keys(&["last_user_message"])
}

#[test]
fn pick_one_passes_only_declared_state_keys() {
    let m = Metrics::new()
        .with("last_user_message", "你好")
        .with("noise", "不该透传");
    let reqs = route().prepare(&m);

    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].state["last_user_message"], "你好");
    assert!(reqs[0].state.get("noise").is_none(), "未声明的事实在请求外");
}

#[test]
fn pick_one_requires_confidence_threshold() {
    let policy = route();
    let qid = policy.primary_qid();

    let low = Metrics::new().with_decisions(Arc::new(DecisionAnswers::new().with(
        qid.clone(),
        DecisionValue::Pick {
            chosen: "trivial".to_string(),
            confidence: 0.5,
            weights: BTreeMap::new(),
        },
    )));
    // 置信不足 = 没有结论（不是「选 needed」）
    assert!(policy.chosen(&low).is_none());
    assert!(!policy.is_triggered(&low));

    let ok = Metrics::new().with_decisions(Arc::new(DecisionAnswers::new().with(
        qid,
        DecisionValue::Pick {
            chosen: "trivial".to_string(),
            confidence: 0.9,
            weights: BTreeMap::new(),
        },
    )));
    let chosen = policy.chosen(&ok).expect("confident choice");
    assert_eq!(chosen.option, "trivial");
    assert!(policy.is_triggered(&ok));
}

#[test]
fn pick_one_without_answer_is_none() {
    assert!(route().chosen(&Metrics::new()).is_none());
}

// ==================== 分级 ====================

fn grade() -> ScorePolicy {
    ScorePolicy::new(
        "test_grade",
        "质量分级",
        "答案质量 ≥ 0.6 时命中",
        "评估答案质量",
        ["很差", "较差", "一般", "较好", "很好"],
        0.6,
    )
}

#[test]
fn score_rejects_out_of_range_levels() {
    // 协议要求 2~10 级：构造错误时宁可零调用，也不发一个必被整批拒的请求
    let one = ScorePolicy::new("t", "t", "d", "i", ["只有一级"], 0.5);
    assert!(one.prepare(&Metrics::new()).is_empty());

    let too_many = ScorePolicy::new(
        "t",
        "t",
        "d",
        "i",
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"],
        0.5,
    );
    assert!(too_many.prepare(&Metrics::new()).is_empty());
}

#[test]
fn score_hits_above_min_score() {
    let policy = grade();
    let qid = policy.primary_qid();

    let build = |score: f32| {
        Metrics::new().with_decisions(Arc::new(DecisionAnswers::new().with(
            qid.clone(),
            DecisionValue::Grade {
                score,
                confidence: 0.9,
            },
        )))
    };

    assert!(!policy.is_triggered(&build(0.4)));
    assert!(policy.is_triggered(&build(0.8)));
    assert_eq!(policy.score(&build(0.8)).map(|g| g.score), Some(0.8));
}

// ==================== 概率 ====================

#[test]
fn presence_hits_above_probability() {
    let policy = PresencePolicy::new(
        "test_presence",
        "存在性",
        "概率 ≥ 0.7 时命中",
        "判断是否需要继续",
        0.7,
    );
    let qid = policy.primary_qid();

    let build = |value: f32| {
        Metrics::new().with_decisions(Arc::new(
            DecisionAnswers::new().with(qid.clone(), DecisionValue::Presence { value }),
        ))
    };

    assert!(!policy.is_triggered(&build(0.5)));
    assert!(policy.is_triggered(&build(0.9)));
    assert_eq!(policy.probability(&build(0.9)), Some(0.9));
    assert!(policy.probability(&Metrics::new()).is_none());
}
