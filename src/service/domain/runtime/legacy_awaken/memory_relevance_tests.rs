//! tests 单元测试（拆分自 memory_relevance.rs）
//!
//! 文件瘦身：原 409 行 → 203 行，测试体 207 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::super::retrieve_node::{KNOWLEDGE_TOP_K, SHORT_TERM_TOP_K};
use super::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionValue, QuestionSpec};
use common::enums::MemoryStatus;

fn node(id: &str, name: &str, desc: &str) -> Memory {
    Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        node_name: name.to_string(),
        node_description: desc.to_string(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        user_tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }))
}

fn short_term(id: &str, summary: &str) -> Memory {
    Memory::new(MemoryPo::ShortTerm(ShortTermMemoryIndexPo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: summary.to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

fn trace(id: &str, output: Option<&str>, input: &str) -> Memory {
    let mut t = MemoryTrace::new(
        "agent-test".to_string(),
        "log-test".to_string(),
        "user-test".to_string(),
        "org-test".to_string(),
        common::enums::MemoryRole::Assistant,
        input.to_string(),
        None,
    );
    t.id = id.to_string();
    t.output = output.map(str::to_string);
    Memory::new(MemoryPo::Trace(t))
}

fn relation(id: &str, relation_type: &str) -> Memory {
    Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: "src".to_string(),
        target_node_id: "dst".to_string(),
        relation_type: relation_type.to_string(),
        weight: None,
        status: common::enums::KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

fn metrics_for(question: &str, cands: &[Memory]) -> Metrics {
    facts(question, cands, None)
}

/// 构造带答案的事实桶（同时带候选：`ranked` 要用它过滤幻觉 id）
fn with_pick(cands: &[Memory], weights: &[(&str, f32)], chosen: &str) -> Metrics {
    let answers = DecisionAnswers::new().with(
        policy(KNOWLEDGE_TOP_K).primary_qid(),
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
    let cands = vec![node("m1", "订单状态机", "订单流转")];
    let m = metrics_for("订单怎么流转", &cands);
    // 一次调用都不该发
    assert!(policy(KNOWLEDGE_TOP_K).prepare(&m).is_empty());
    assert!(!policy(KNOWLEDGE_TOP_K).is_triggered(&m));
}

#[test]
fn request_puts_every_candidate_into_one_question() {
    let cands = vec![
        node("m1", "订单状态机", "订单从创建到完成"),
        node("m2", "退款流程", "上次讨论过退款流程"),
    ];
    let m = metrics_for("订单怎么流转", &cands);
    let p = policy(KNOWLEDGE_TOP_K);
    let reqs = p.prepare(&m);

    // 一次请求、一道题：不是每条候选各问一次
    assert_eq!(reqs.len(), 1);
    match &reqs[0].questions[&p.primary_qid()] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options.len(), 2);
            assert!(options["m1"].contains("订单状态机"));
            assert!(options["m2"].contains("退款"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
    assert_eq!(reqs[0].scene, SCENE);
    assert_eq!(reqs[0].state["candidate_count"], 2);
}

#[test]
fn candidates_without_snippet_do_not_become_options() {
    let cands = vec![
        node("m1", "A", ""),
        node("m2", "B", "b"),
        node("m3", "C", "c"),
    ];
    let m = metrics_for("q", &cands);
    let p = policy(KNOWLEDGE_TOP_K);
    let reqs = p.prepare(&m);
    match &reqs[0].questions[&p.primary_qid()] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options.len(), 2, "空判据不占选项位");
            assert!(!options.contains_key("m1"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
}

#[test]
fn no_answer_falls_back_to_old_path() {
    let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
    let m = metrics_for("q", &cands);
    // 有请求但没答案 → 未命中，`ranked` 给 None（不是「全不相关」）
    assert!(!policy(KNOWLEDGE_TOP_K).is_triggered(&m));
    assert!(policy(KNOWLEDGE_TOP_K).ranked(&m).is_none());
}

#[test]
fn usable_pick_triggers_the_policy() {
    let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
    let m = with_pick(&cands, &[("m1", 0.6), ("m2", 0.4)], "m1");
    assert_eq!(
        policy(KNOWLEDGE_TOP_K).evaluate(&m),
        vec![SCENE.to_string()]
    );
}

#[test]
fn empty_weights_do_not_trigger() {
    let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
    let m = with_pick(&cands, &[], "m1");
    assert!(!policy(KNOWLEDGE_TOP_K).is_triggered(&m));
}

#[test]
fn ranked_keeps_candidates_above_threshold() {
    let cands = vec![
        node("m1", "A", "a"),
        node("m2", "B", "b"),
        node("m3", "C", "c"),
    ];
    let m = with_pick(&cands, &[("m1", 0.02), ("m2", 0.6), ("m3", 0.38)], "m2");

    // 按权重降序，且 m1 低于阈值被剔除
    assert_eq!(
        policy(KNOWLEDGE_TOP_K).ranked(&m),
        Some(vec!["m2".to_string(), "m3".to_string()])
    );
}

#[test]
fn chosen_survives_even_when_every_weight_is_low() {
    let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
    let m = with_pick(&cands, &[("m1", 0.01), ("m2", 0.02)], "m1");

    // 全批低于阈值也只保留被选中的那条，绝不清空召回
    assert_eq!(
        policy(KNOWLEDGE_TOP_K).ranked(&m),
        Some(vec!["m1".to_string()])
    );
}

#[test]
fn unknown_ids_from_model_are_dropped() {
    let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
    let m = with_pick(&cands, &[("m1", 0.2), ("ghost", 0.9)], "m1");

    assert_eq!(
        policy(KNOWLEDGE_TOP_K).ranked(&m),
        Some(vec!["m1".to_string()])
    );
}

#[test]
fn keep_top_k_is_bounded() {
    let cands: Vec<Memory> = (0..10).map(|i| node(&format!("m{i}"), "n", "d")).collect();
    let weights: Vec<(&str, f32)> = vec![
        ("m0", 0.9),
        ("m1", 0.8),
        ("m2", 0.7),
        ("m3", 0.6),
        ("m4", 0.5),
        ("m5", 0.4),
        ("m6", 0.3),
        ("m7", 0.2),
        ("m8", 0.1),
        ("m9", 0.9),
    ];
    let m = with_pick(&cands, &weights, "m0");

    let kept = policy(KNOWLEDGE_TOP_K).ranked(&m).expect("selection");
    assert_eq!(kept.len(), KNOWLEDGE_TOP_K);
}

/// 短期记忆复用同一排序形态，但保留上限必须是自己的 3 条而非长期知识的 5 条
#[test]
fn short_term_rerank_uses_its_own_keep_budget() {
    let cands: Vec<Memory> = (0..10).map(|i| node(&format!("st{i}"), "n", "d")).collect();
    let weights = vec![
        ("st0", 0.95),
        ("st1", 0.9),
        ("st2", 0.85),
        ("st3", 0.8),
        ("st4", 0.7),
        ("st5", 0.6),
        ("st6", 0.5),
        ("st7", 0.4),
        ("st8", 0.3),
        ("st9", 0.2),
    ];
    let m = with_pick(&cands, &weights, "st0");

    // 长期 5 / 短期 3 是两路各自的渲染预算，不能共用同一个 keep。
    let kept = policy(SHORT_TERM_TOP_K).ranked(&m).expect("selection");
    assert_eq!(kept.len(), SHORT_TERM_TOP_K);
    assert_eq!(
        kept.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["st0", "st1", "st2"]
    );
}

/// 四类候选各自带来源标签（逐字取值；R48：用字面量而非常量断言，
/// 否则改常量会连测试一起漂移，「取值正确」就不可判定）
#[test]
fn snippet_labels_each_source_verbatim() {
    assert_eq!(
        snippet(&node("m1", "订单状态机", "订单从创建到完成"), None),
        "[知识节点]订单状态机：订单从创建到完成"
    );
    assert_eq!(
        snippet(&short_term("st1", "会议结论：下周上线"), None),
        "[短期记忆]会议结论：下周上线"
    );
    assert_eq!(
        snippet(&trace("t1", Some("思考输出正文"), "输入正文"), None),
        "[trace]思考输出正文"
    );
    assert_eq!(snippet(&relation("r1", "depends"), None), "[关系]depends");
}

/// 标签不得把空判据变成可判据（否则空候选会白占一个选项位）
#[test]
fn empty_body_stays_empty_even_with_label() {
    assert_eq!(
        snippet(&node("m1", "有名字", ""), None),
        "",
        "知识节点 body 为空"
    );
    assert_eq!(snippet(&short_term("st1", ""), None), "");
    assert_eq!(snippet(&short_term("st1", "   "), None), "");
    assert_eq!(snippet(&trace("t1", Some(""), ""), None), "");
    assert_eq!(snippet(&relation("r1", ""), None), "");
}

/// 成对不变量：①选项值 == `snippet()` 逐字（标签确实进了选项）
/// ②`state` 仍只有 `question` + `candidate_count` 两键（没被顺带扩成三键）
#[test]
fn options_carry_the_label_and_state_keeps_its_two_keys() {
    let cands = vec![
        node("m1", "订单状态机", "订单流转"),
        short_term("st1", "会议结论"),
    ];
    let m = metrics_for("订单怎么流转", &cands);
    let p = policy(KNOWLEDGE_TOP_K);
    let reqs = p.prepare(&m);
    match &reqs[0].questions[&p.primary_qid()] {
        QuestionSpec::Pick { options, .. } => {
            assert_eq!(options["m1"], snippet(&cands[0], None));
            assert_eq!(options["st1"], snippet(&cands[1], None));
            assert!(options["m1"].starts_with("[知识节点]"));
            assert!(options["st1"].starts_with("[短期记忆]"));
        }
        other => panic!("expected Pick, got {other:?}"),
    }
    let state = reqs[0]
        .state
        .as_object()
        .expect("state must be a JSON object");
    assert_eq!(
        state.len(),
        2,
        "state 只应有 question + candidate_count：{state:?}"
    );
    assert_eq!(state["candidate_count"], 2);
}
