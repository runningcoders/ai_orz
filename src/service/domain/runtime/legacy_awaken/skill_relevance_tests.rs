//! tests 单元测试（拆分自 skill_relevance.rs）
//!
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。

use super::super::retrieve_node::METHOD_SKILLS_TOP_K;
use super::*;
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionValue, QuestionSpec};

fn skill(id: &str, name: &str, desc: &str, category: &str) -> SkillPo {
    SkillPo {
        id: id.to_string(),
        name: name.to_string(),
        description: desc.to_string(),
        category: category.to_string(),
        ..Default::default()
    }
}

fn metrics_for(question: &str, cands: &[SkillPo]) -> Metrics {
    facts(question, cands)
}

/// 构造带答案的事实桶（同时带候选：`ranked` 要用它过滤幻觉 id）
fn with_pick(cands: &[SkillPo], weights: &[(&str, f32)], chosen: &str) -> Metrics {
    let answers = DecisionAnswers::new().with(
        policy(METHOD_SKILLS_TOP_K).primary_qid(),
        DecisionValue::Pick {
            chosen: chosen.to_string(),
            confidence: 0.9,
            weights: weights
                .iter()
                .map(|(id, w)| ((*id).to_string(), *w))
                .collect(),
        },
    );
    metrics_for("q", cands).with_decisions(Arc::new(answers))
}

#[test]
fn fewer_than_two_candidates_skips_the_call() {
    let cands = vec![skill(
        "s1",
        "反过度工程",
        "评估该不该写这么多",
        "coding_standards",
    )];
    let m = metrics_for("要不要加这一层抽象", &cands);
    // 一次调用都不该发
    assert!(policy(METHOD_SKILLS_TOP_K).prepare(&m).is_empty());
    assert!(!policy(METHOD_SKILLS_TOP_K).is_triggered(&m));
}

#[test]
fn request_puts_every_skill_into_one_question() {
    let cands = vec![
        skill("s1", "反过度工程", "评估该不该写这么多", "coding_standards"),
        skill(
            "s2",
            "Rust 编码最佳实践",
            "写 Rust 时逐条对照",
            "coding_standards",
        ),
    ];
    let m = metrics_for("这段 Rust 怎么写", &cands);
    let p = policy(METHOD_SKILLS_TOP_K);
    let reqs = p.prepare(&m);

    // 一次请求、一道题：不是每条候选各问一次
    assert_eq!(reqs.len(), 1);
    match &reqs[0].questions[&p.primary_qid()] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options.len(), 2);
            // 判据文本 = 名称 + 何时用 + 分类
            assert!(options["s1"].contains("反过度工程"));
            assert!(options["s1"].contains("coding_standards"));
            assert!(options["s2"].contains("Rust"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
    assert_eq!(reqs[0].scene, SCENE);
    assert_eq!(reqs[0].state["candidate_count"], 2);
}

#[test]
fn skills_without_criteria_do_not_become_options() {
    let cands = vec![
        skill("s1", "", "", ""),
        skill("s2", "A", "a", ""),
        skill("s3", "B", "b", ""),
    ];
    let m = metrics_for("q", &cands);
    let p = policy(METHOD_SKILLS_TOP_K);
    let reqs = p.prepare(&m);
    match &reqs[0].questions[&p.primary_qid()] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options.len(), 2, "空判据不占选项位");
            assert!(!options.contains_key("s1"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
}

#[test]
fn no_answer_falls_back_to_old_path() {
    let cands = vec![skill("s1", "A", "a", ""), skill("s2", "B", "b", "")];
    let m = metrics_for("q", &cands);
    // 有请求但没答案 → 未命中，`ranked` 给 None（不是「全不相关」）
    assert!(!policy(METHOD_SKILLS_TOP_K).is_triggered(&m));
    assert!(policy(METHOD_SKILLS_TOP_K).ranked(&m).is_none());
}

#[test]
fn usable_pick_triggers_the_policy() {
    let cands = vec![skill("s1", "A", "a", ""), skill("s2", "B", "b", "")];
    let m = with_pick(&cands, &[("s1", 0.6), ("s2", 0.4)], "s1");
    assert_eq!(
        policy(METHOD_SKILLS_TOP_K).evaluate(&m),
        vec![SCENE.to_string()]
    );
}

#[test]
fn empty_weights_do_not_trigger() {
    let cands = vec![skill("s1", "A", "a", ""), skill("s2", "B", "b", "")];
    let m = with_pick(&cands, &[], "s1");
    assert!(!policy(METHOD_SKILLS_TOP_K).is_triggered(&m));
}

#[test]
fn ranked_keeps_skills_above_threshold() {
    let cands = vec![
        skill("s1", "A", "a", ""),
        skill("s2", "B", "b", ""),
        skill("s3", "C", "c", ""),
    ];
    let m = with_pick(&cands, &[("s1", 0.02), ("s2", 0.6), ("s3", 0.38)], "s2");

    // 按权重降序，且 s1 低于阈值被剔除
    assert_eq!(
        policy(METHOD_SKILLS_TOP_K).ranked(&m),
        Some(vec!["s2".to_string(), "s3".to_string()])
    );
}

#[test]
fn chosen_survives_even_when_every_weight_is_low() {
    let cands = vec![skill("s1", "A", "a", ""), skill("s2", "B", "b", "")];
    let m = with_pick(&cands, &[("s1", 0.01), ("s2", 0.02)], "s1");

    // 全批低于阈值也只保留被选中的那条，绝不清空召回
    assert_eq!(
        policy(METHOD_SKILLS_TOP_K).ranked(&m),
        Some(vec!["s1".to_string()])
    );
}

#[test]
fn unknown_ids_from_model_are_dropped() {
    let cands = vec![skill("s1", "A", "a", ""), skill("s2", "B", "b", "")];
    let m = with_pick(&cands, &[("s1", 0.2), ("ghost", 0.9)], "s1");

    assert_eq!(
        policy(METHOD_SKILLS_TOP_K).ranked(&m),
        Some(vec!["s1".to_string()])
    );
}

#[test]
fn keep_top_k_is_bounded() {
    let cands: Vec<SkillPo> = (0..8)
        .map(|i| skill(&format!("s{i}"), "n", "d", ""))
        .collect();
    let weights: Vec<(&str, f32)> = vec![
        ("s0", 0.9),
        ("s1", 0.8),
        ("s2", 0.7),
        ("s3", 0.6),
        ("s4", 0.5),
        ("s5", 0.4),
        ("s6", 0.3),
        ("s7", 0.2),
    ];
    let m = with_pick(&cands, &weights, "s0");

    let kept = policy(METHOD_SKILLS_TOP_K).ranked(&m).expect("selection");
    assert_eq!(kept.len(), METHOD_SKILLS_TOP_K);
}
