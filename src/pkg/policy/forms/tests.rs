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

#[test]
fn option_threshold_overrides_global_per_option() {
    // 全局 0.85，但 trivial 单列 0.2、needed 单列 0.95
    let policy = route()
        .with_option_min_confidence("trivial", 0.2)
        .with_option_min_confidence("needed", 0.95);
    assert_eq!(policy.option_threshold("trivial"), Some(0.2));
    assert_eq!(policy.option_threshold("needed"), Some(0.95));
    // 未单列的选项落回全局门槛；不在选项集内的是幻觉选项
    assert_eq!(policy.option_threshold("unknown"), None);
}

#[test]
fn option_threshold_falls_back_to_global_value() {
    let policy = PickOnePolicy::new(
        "test_route",
        "路由",
        "判断是否直回",
        "选择一种处理方式",
        [("trivial", "琐碎，可直回"), ("needed", "需要完整思考")],
        0.85,
    );
    assert_eq!(policy.option_threshold("trivial"), Some(0.85));
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
fn score_rejects_only_protocol_out_of_range_levels() {
    // 边界唯一真相是 common::api::cerebellum 的 2~255，不是写死的 2~10。
    // 构造错误时宁可零调用，也不发一个必被整批拒的请求。
    let one = ScorePolicy::new("t", "t", "d", "i", ["只有一级"], 0.5);
    assert!(one.prepare(&Metrics::new()).is_empty());

    let too_many = ScorePolicy::new("t", "t", "d", "i", vec!["L"; 256], 0.5);
    assert!(too_many.prepare(&Metrics::new()).is_empty());

    // 反向护栏：11 级曾被旧上限 10 误杀；255 级是协议真实上限，必须放行
    let eleven = ScorePolicy::new("t", "t", "d", "i", vec!["L"; 11], 0.5);
    assert_eq!(eleven.prepare(&Metrics::new()).len(), 1);
    let at_protocol_ceiling = ScorePolicy::new("t", "t", "d", "i", vec!["L"; 255], 0.5);
    assert_eq!(at_protocol_ceiling.prepare(&Metrics::new()).len(), 1);
}

/// 造一个 score 题答案（协议原值是 0-based 等级序号的加权期望）
fn grade_metrics(qid: &str, raw: f32, confidence: f32) -> Metrics {
    Metrics::new().with_decisions(Arc::new(DecisionAnswers::new().with(
        qid.to_string(),
        DecisionValue::Grade {
            score: raw,
            confidence,
        },
    )))
}

#[test]
fn score_hits_above_min_score() {
    let policy = grade(); // 5 级 ⇒ 原值值域 0~4；upper 缺省 1.0 ⇒ 读回 0~1
    let qid = policy.primary_qid();

    // 1.6/4 = 0.4 < 0.6；3.2/4 = 0.8 ≥ 0.6
    assert!(!policy.is_triggered(&grade_metrics(&qid, 1.6, 0.9)));
    assert!(policy.is_triggered(&grade_metrics(&qid, 3.2, 0.9)));
    assert_eq!(
        policy
            .score(&grade_metrics(&qid, 3.2, 0.9))
            .map(|g| g.score),
        Some(3.2),
        "score() 给协议原值，不做缩放"
    );
    assert_eq!(
        policy
            .scaled(&grade_metrics(&qid, 3.2, 0.9))
            .map(|g| g.score),
        Some(0.8),
        "scaled() 才给业务刻度"
    );
}

/// 生产反事实：11 级语义等级题的真实端点响应 `score=6.95, confidence=0.79`
/// （2026-11-11 实测）。服务端已加权完，客户端一次线性缩放即可。
#[test]
fn score_scales_server_judgement_to_arbitrary_upper() {
    // qid 只由场景 id 决定，换 upper 不换题面
    let with_upper = |upper: f32| {
        ScorePolicy::new("t", "t", "d", "评估强度", vec!["L"; 11], 0.0).with_upper(upper)
    };
    let qid = with_upper(1.0).primary_qid();
    let m = grade_metrics(&qid, 6.95, 0.79);

    assert!(
        (with_upper(100.0).scaled(&m).expect("应读出分数").score - 69.5).abs() < 1e-4,
        "6.95 / 10 × 100 = 69.5"
    );
    assert!((with_upper(10.0).scaled(&m).unwrap().score - 6.95).abs() < 1e-4);
    assert!((with_upper(1.0).scaled(&m).unwrap().score - 0.695).abs() < 1e-4);
}

#[test]
fn score_confidence_is_observation_only() {
    // 连续量不做单档置信门禁：低置信也要读出分数，不能折叠成「没结论」
    let policy = grade();
    let qid = policy.primary_qid();
    let low = grade_metrics(&qid, 3.2, 0.18);
    assert_eq!(policy.scaled(&low).map(|g| g.score), Some(0.8));
    assert!((policy.scaled(&low).unwrap().confidence - 0.18).abs() < 1e-6);
}

#[test]
fn score_clamps_out_of_range_and_rejects_bad_upper() {
    let policy = grade();
    let qid = policy.primary_qid();
    assert_eq!(
        policy
            .scaled(&grade_metrics(&qid, 99.0, 0.9))
            .map(|g| g.score),
        Some(1.0),
        "服务端概率和 ≠ 1 导致越界时夹紧，不得溢出业务上限"
    );
    assert_eq!(
        policy
            .scaled(&grade_metrics(&qid, -1.0, 0.9))
            .map(|g| g.score),
        Some(0.0)
    );
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let bad_policy = grade().with_upper(bad);
        assert!(
            bad_policy.scaled(&grade_metrics(&qid, 3.2, 0.9)).is_none(),
            "非法 upper 必须当没结论，不能静默出 0 分：{bad}"
        );
    }
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
