//! `legacy_settle` 蓝图的拓扑不变量测试（纯静态，不碰 I/O）
//!
//! 与 `integration_tests.rs` 的分工：
//!
//! | 文件 | 测什么 |
//! |------|--------|
//! | 本文件 | 图**长什么样**：拓扑、预算、出口信号与边的双向一致 |
//! | `integration_tests.rs` | 图**跑起来怎样**：真装配 + 真落库 + 脚本后端 |
//!
//! 这些约束在老 `sleep_and_settle` 里全藏在 `match` 分支里，漏一个分支就是
//! 静默丢路径；迁移成显式的边之后，可以被断言。

use super::blueprint::{MAX_SETTLE_BATCHES, legacy_settle_blueprint};
use super::nodes::{
    NODE_FINALIZE, NODE_PREPARE, NODE_SETTLE, SIGNAL_BLOCKED, SIGNAL_EMPTY, SIGNAL_NEXT,
    SIGNAL_READY,
};
use crate::pkg::think_path::ThinkPathBlueprint;

fn blueprint() -> ThinkPathBlueprint {
    legacy_settle_blueprint().expect("legacy_settle blueprint")
}

fn edges_from(bp: &ThinkPathBlueprint, from: &str) -> Vec<(String, Option<String>)> {
    bp.edges
        .iter()
        .filter(|e| e.from == from)
        .map(|e| (e.to.clone(), e.on.clone()))
        .collect()
}

fn budget_of(bp: &ThinkPathBlueprint, id: &str) -> Option<usize> {
    bp.nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.budget.max_activations)
}

#[test]
fn entry_and_nodes_are_declared() {
    let bp = blueprint();
    assert_eq!(bp.entry, NODE_PREPARE);
    let ids: Vec<&str> = bp.nodes.iter().map(|n| n.id.as_str()).collect();
    for expected in [NODE_PREPARE, NODE_SETTLE, NODE_FINALIZE] {
        assert!(ids.contains(&expected), "{expected} missing: {ids:?}");
    }
    assert_eq!(ids.len(), 3, "不应有多余节点: {ids:?}");
}

/// 终止性护栏：每个节点都必须显式声明激活预算
///
/// 图里有环，没有静态环检测，预算是唯一的停车手段（默认是「只跑一次」）。
#[test]
fn every_node_declares_an_activation_budget() {
    for node in &blueprint().nodes {
        assert!(
            node.budget.max_activations.is_some(),
            "node {} has no explicit max_activations",
            node.id
        );
    }
}

/// 回环预算：`prepare` 比 `settle` 多一次，用来发现「队列已空」
#[test]
fn loop_budgets_leave_one_extra_prepare_to_discover_empty() {
    let bp = blueprint();
    assert_eq!(budget_of(&bp, NODE_PREPARE), Some(MAX_SETTLE_BATCHES + 1));
    assert_eq!(budget_of(&bp, NODE_SETTLE), Some(MAX_SETTLE_BATCHES));
    assert_eq!(budget_of(&bp, NODE_FINALIZE), Some(1));
}

/// `prepare` 两个出口都必须有边：有批次 → settle，无批次 → finalize
#[test]
fn both_prepare_exits_have_edges() {
    let outs = edges_from(&blueprint(), NODE_PREPARE);
    for (signal, target) in [(SIGNAL_READY, NODE_SETTLE), (SIGNAL_EMPTY, NODE_FINALIZE)] {
        assert!(
            outs.iter()
                .any(|(to, on)| to == target && on.as_deref() == Some(signal)),
            "prepare→{target} on `{signal}` missing: {outs:?}"
        );
    }
}

/// `settle` 两个出口都必须有边：仅 Final 回环，其余一律进收尾
#[test]
fn settle_exits_next_back_to_prepare_and_blocked_to_finalize() {
    let outs = edges_from(&blueprint(), NODE_SETTLE);
    for (signal, target) in [(SIGNAL_NEXT, NODE_PREPARE), (SIGNAL_BLOCKED, NODE_FINALIZE)] {
        assert!(
            outs.iter()
                .any(|(to, on)| to == target && on.as_deref() == Some(signal)),
            "settle→{target} on `{signal}` missing: {outs:?}"
        );
    }
}

/// 回到 `prepare` 的边只有 `settle -next->` 一条（唯一回环）
#[test]
fn only_settle_loops_back_to_prepare() {
    let bp = blueprint();
    let back: Vec<&str> = bp
        .edges
        .iter()
        .filter(|e| e.to == NODE_PREPARE)
        .map(|e| e.from.as_str())
        .collect();
    assert_eq!(back, vec![NODE_SETTLE]);
}

/// 从任意节点出发都必须能走到 `finalize`：否则引擎会因「无合法候选」静默结束
#[test]
fn every_route_reaches_finalize() {
    let bp = blueprint();
    let reachable_from = |start: &str| -> Vec<String> {
        let mut seen = vec![start.to_string()];
        let mut queue = vec![start.to_string()];
        while let Some(current) = queue.pop() {
            for (to, _) in edges_from(&bp, &current) {
                if !seen.contains(&to) {
                    seen.push(to.clone());
                    queue.push(to);
                }
            }
        }
        seen
    };
    for start in [NODE_PREPARE, NODE_SETTLE] {
        assert!(
            reachable_from(start).contains(&NODE_FINALIZE.to_string()),
            "{start} cannot reach {NODE_FINALIZE}"
        );
    }
}

/// `max_steps` 必须覆盖「每批两步 + 最后一次空 prepare + finalize」
#[test]
fn max_steps_covers_every_loop_iteration() {
    let bp = blueprint();
    assert_eq!(bp.limits.max_steps, MAX_SETTLE_BATCHES * 2 + 2);
    assert_eq!(bp.limits.timeout_secs, 0);
    assert_eq!(bp.limits.advisor_calls_budget, 0);
}
