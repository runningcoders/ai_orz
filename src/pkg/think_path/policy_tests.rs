//! 准入策略集测试：预算封顶 / 前驱与信号匹配 / 未准入原因可解释
//!
//! 这些用例锁住的是**策略语义**，不是引擎流程：
//! 新增准入形态时只需加一个 `Policy` + 一个用例，仲裁器主循环不动。

use super::arbiter::Arbiter;
use super::blueprint::ThinkPathBuilder;
use super::bus::ProviderSet;
use super::engine::ThinkPathEngine;
use super::node::{ActivationBudget, ActivationRule, BusPredicate};
use super::policy::{
    BudgetPolicy, BusPredicatePolicy, InSetPolicy, M_ACTIVATIONS, M_COOLDOWN_STEPS,
    M_MAX_ACTIVATIONS, M_PREV_NODE, M_SIGNAL, M_SINCE_LAST, bus_depth_key, bus_loaded_key,
    compile_guard,
};
use super::testkit::ProbeNode;
use super::types::{ContextKey, LoadDepth, NodeId, NodeSignal, PhaseId, Stimulus};
use crate::pkg::policy::{Metrics, Policy};
use std::sync::Arc;

fn stimulus() -> Stimulus {
    Stimulus {
        source: "test".to_string(),
        text: "看下这个".to_string(),
        message_id: None,
    }
}

fn budget(max_activations: usize, cooldown_steps: usize) -> ActivationBudget {
    ActivationBudget {
        max_activations,
        cooldown_steps,
    }
}

/// 预算算子：次数 / 距上次步数 / 上限 / 冷却
///
/// ⚠️ 上限与冷却**必须**与构造策略时的预算一致：策略从算子取实际值，
/// 构建期预算只作兜底。夹具自相矛盾会得出「冷却未生效」的假象。
fn budget_metrics(activations: u64, since_last: u64, max: u64, cooldown: u64) -> Metrics {
    Metrics::new()
        .with(M_ACTIVATIONS, activations)
        .with(M_SINCE_LAST, since_last)
        .with(M_MAX_ACTIVATIONS, max)
        .with(M_COOLDOWN_STEPS, cooldown)
}

#[tokio::test]
async fn budget_policy_caps_activations() {
    let policy = BudgetPolicy::new(&NodeId::new("deliberate"), &budget(2, 0));
    assert!(policy.is_triggered(&budget_metrics(0, u64::MAX, 2, 0)));
    assert!(policy.is_triggered(&budget_metrics(1, u64::MAX, 2, 0)));
    // 达上限即不命中：这是终止性的落点，不能交给概率
    assert!(!policy.is_triggered(&budget_metrics(2, u64::MAX, 2, 0)));
}

#[tokio::test]
async fn budget_policy_enforces_cooldown() {
    let policy = BudgetPolicy::new(&NodeId::new("triage"), &budget(9, 3));
    // 刚跑过（距上次 2 步，冷却 3 步）→ 冷却未过，不命中
    assert!(!policy.is_triggered(&budget_metrics(1, 2, 9, 3)));
    // 冷却刚过
    assert!(policy.is_triggered(&budget_metrics(1, 4, 9, 3)));
    // 从未激活：since_last = u64::MAX，冷却不构成约束
    assert!(policy.is_triggered(&budget_metrics(0, u64::MAX, 9, 3)));
}

#[tokio::test]
async fn inset_policy_matches_predecessor_and_signal() {
    let node = NodeId::new("deliberate");
    let after = InSetPolicy::after(&node, vec![NodeId::new("prepare")]);
    assert!(after.is_triggered(&Metrics::new().with(M_PREV_NODE, "prepare")));
    assert!(!after.is_triggered(&Metrics::new().with(M_PREV_NODE, "consolidate")));
    // 算子缺失（首步，没有前驱）→ 不命中，而不是放行
    assert!(!after.is_triggered(&Metrics::new()));

    let signal = InSetPolicy::signal(&node, vec!["done".to_string(), "overflow".to_string()]);
    assert!(signal.is_triggered(&Metrics::new().with(M_SIGNAL, "overflow")));
    assert!(!signal.is_triggered(&Metrics::new().with(M_SIGNAL, "fail")));
}

#[tokio::test]
async fn bus_predicate_policy_reads_projected_metrics() {
    let loaded = BusPredicatePolicy::from_predicate(&BusPredicate::KeyLoaded(ContextKey::Memory));
    assert!(loaded.is_triggered(&Metrics::new().with(&bus_loaded_key(ContextKey::Memory), true)));
    assert!(!loaded.is_triggered(&Metrics::new().with(&bus_loaded_key(ContextKey::Memory), false)));

    let deep = BusPredicatePolicy::from_predicate(&BusPredicate::KeyAtLeast(
        ContextKey::Memory,
        LoadDepth::Full,
    ));
    let key = bus_depth_key(ContextKey::Memory);
    // 深度档位：0 未加载 / 1 Shallow / 2 Full
    assert!(!deep.is_triggered(&Metrics::new().with(&key, 1u64)));
    assert!(deep.is_triggered(&Metrics::new().with(&key, 2u64)));
}

#[tokio::test]
async fn compiled_guard_explains_why_a_node_was_not_admitted() {
    let bp = ThinkPathBuilder::new("t_policy")
        .entry("prepare")
        .node_with("prepare", PhaseId::Prepare, 0, "")
        .node_with("deliberate", PhaseId::Deliberate, 0, "")
        .edge("prepare", "deliberate", Some("done"))
        .build()
        .expect("blueprint");

    let engine = ThinkPathEngine::new(
        bp,
        vec![
            Arc::new(ProbeNode::new("prepare", NodeSignal::Done)),
            Arc::new(ProbeNode::new("deliberate", NodeSignal::Done)),
        ],
        Arbiter::deterministic(),
        ProviderSet::new(),
    )
    .expect("engine");

    // 静态边编译成了目标节点的准入条件，且描述可读
    let describe = engine
        .pool()
        .guard(&NodeId::new("deliberate"))
        .expect("guard")
        .describe();
    assert!(describe.contains("prev_node ∈ [prepare]"), "{describe}");
    assert!(describe.contains("signal ∈ [done]"), "{describe}");

    let run = engine.run("rp", stimulus()).await.expect("run");
    assert_eq!(run.trace.steps, 2);

    // 跑完之后没有任何节点可再被准入，且原因说得清楚（不是只返回空候选集）
    let last = (NodeId::new("deliberate"), NodeSignal::Done);
    let rejections =
        Arbiter::deterministic().rejections(engine.pool(), &run, Some((&last.0, &last.1)));
    assert_eq!(rejections.len(), 2, "{rejections:?}");
    let all = rejections
        .iter()
        .flat_map(|(_, reasons)| reasons.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(all.contains("激活预算"), "{all}");
}

#[tokio::test]
async fn empty_rule_compiles_to_budget_only_guard() {
    let guard = compile_guard(
        &NodeId::new("orphan"),
        &ActivationRule::default(),
        &budget(1, 0),
    );
    // 无 after / on / when → 只有预算一条；空集合不生成恒假策略
    assert_eq!(guard.required_metrics().len(), 4);
    assert!(guard.describe().contains("activations < 1"));
}
