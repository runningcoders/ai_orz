//! `legacy_awaken` 蓝图的拓扑不变量测试
//!
//! 这些用例不碰 I/O，只锁住「图本身是否还成立」——老链路里这些约束
//! 全部藏在 `match think_result` 的分支里，漏一个分支就是静默丢路径；
//! 现在是显式的边，可以被断言。

use super::blueprint::{MAX_LOOP_ROUNDS, legacy_awaken_blueprint};
use super::nodes::{EXIT_MAX_ROUNDS, map_think_result, sample_results};
use crate::pkg::think_path::ThinkPathBlueprint;
use crate::service::domain::runtime::types::ThinkLoopResult;

fn blueprint() -> ThinkPathBlueprint {
    legacy_awaken_blueprint().expect("legacy_awaken blueprint")
}

fn edges_from(bp: &ThinkPathBlueprint, from: &str) -> Vec<(String, Option<String>)> {
    bp.edges
        .iter()
        .filter(|e| e.from == from)
        .map(|e| (e.to.clone(), e.on.clone()))
        .collect()
}

#[test]
fn entry_and_nodes_are_declared() {
    let bp = blueprint();
    assert_eq!(bp.entry, "prepare");
    let ids: Vec<&str> = bp.nodes.iter().map(|n| n.id.as_str()).collect();
    for expected in [
        "prepare",
        "deliberate",
        "compact",
        "force_compact",
        "consolidate",
    ] {
        assert!(ids.contains(&expected), "{expected} missing: {ids:?}");
    }
}

/// 终止性护栏：每个节点都必须显式声明激活预算
///
/// 图 + 回环没有静态环检测，预算是唯一的停车手段（且默认是「只跑一次」）。
/// 新增节点而忘记声明预算，是「上线后跑不完」的最常见原因。
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

#[test]
fn loop_nodes_budget_is_explicitly_loosened() {
    let bp = blueprint();
    let budget_of = |id: &str| -> Option<usize> {
        bp.nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.budget.max_activations)
    };
    // 回环上的节点必须允许多次出现；其余默认一次。
    // ⚠️ 断言的是常量本身而不是抄下来的字面量 —— 抄一份数字到这里就是第三份真相：
    // 改了蓝图而这里没改，测试照样绿，运行时却在别处静默停车。
    assert_eq!(budget_of("prepare"), Some(MAX_LOOP_ROUNDS));
    assert_eq!(budget_of("deliberate"), Some(MAX_LOOP_ROUNDS));
    assert_eq!(
        budget_of("compact"),
        Some(MAX_LOOP_ROUNDS.saturating_sub(1))
    );
    assert_eq!(budget_of("consolidate"), Some(1));
}

/// `Deliberate` 可能产出的每个信号都必须有一条从它出发的边
///
/// ⚠️ 这里**从真实映射函数穷举**，不用手写字面量数组。
/// 老写法是把 `["done","overflow","max_rounds","cancelled"]` 抄一份到测试里 ——
/// 那是第三份真相：改了 `map_think_result` 而测试没跟着改，断言照样绿，
/// 运行时却因无候选而静默终止（不写 Trace、不报统计、不发事件）。
#[test]
fn every_deliberate_exit_signal_has_an_edge() {
    let bp = blueprint();
    let outs = edges_from(&bp, "deliberate");
    for result in sample_results(2) {
        let mapped = map_think_result(result, 2);
        let tag = mapped.signal.tag().to_string();
        assert!(
            outs.iter()
                .any(|(_, on)| on.as_deref() == Some(tag.as_str())),
            "deliberate yields signal `{tag}` but no edge declares it: {outs:?}"
        );
    }
}

/// 反向：蓝图声明的每条 `deliberate` 出边，都必须有手段真的产出这个 Tag
///
/// 只测「产出都有边」还不够 —— 反过来，「边存在但没人产出」同样危险：
/// 那意味着配置了一条永远走不到的路径，压缩或收尾逻辑形同虚设。
#[test]
fn every_deliberate_edge_signal_is_reachable() {
    let bp = blueprint();
    let produced: Vec<String> = sample_results(2)
        .into_iter()
        .map(|r| map_think_result(r, 2).signal.tag().to_string())
        .collect();
    for (to, on) in edges_from(&bp, "deliberate") {
        let Some(tag) = on else { continue };
        assert!(
            produced.contains(&tag),
            "edge deliberate→{to} declares signal `{tag}` but no ThinkLoopResult yields it: {produced:?}"
        );
    }
}

/// `Artifacts.exit` 是 `Consolidate` 的唯一分支依据：它必须能消化的每个值都自洽
///
/// `Consolidate` 目前只特判 `final` 与 `cancelled`，其余走通用分支。
/// 关键在于「哪些 exit 真的会被送到 `Consolidate`」——这是由图拓扑决定的：
/// `overflow` 必须先过 `compact → prepare` 回环，不可能直达收尾。
#[test]
fn consolidate_only_sees_exits_the_graph_can_deliver() {
    let bp = blueprint();
    // 能直达 consolidate 的前驱
    let direct: Vec<&str> = bp
        .edges
        .iter()
        .filter(|e| e.to == "consolidate")
        .map(|e| e.from.as_str())
        .collect();
    assert!(
        direct.contains(&"deliberate"),
        "deliberate should feed consolidate directly"
    );
    assert!(
        direct.contains(&"force_compact"),
        "force_compact should feed consolidate"
    );
    // `compact` 走的是回环回 prepare，不直达收尾：因此 `Consolidate`
    // 永远不会看到 `overflow`（它会先变成下一轮的 prepare→deliberate）
    assert!(
        !direct.contains(&"compact"),
        "compact reaches consolidate only via the prepare loop"
    );

    // ForceCompact 会把 exit 改写成 max_rounds，这是 Consolidate 必见的一个值。
    // 断言的是**监控口径**：Trace 元数据用原值、统计事件用小写，合起来必须与
    // 老 `awaken` 完全一致 —— 否则同一个退出原因会在看板上分裂成两个标签。
    assert_eq!(EXIT_MAX_ROUNDS, "MaxRoundsExceeded");
    assert_eq!(EXIT_MAX_ROUNDS.to_lowercase(), "maxroundsexceeded");
}

/// 轮次记账：`overflow` 必须把本轮消耗累加到总数上，否则回环会无限重开
#[test]
fn overflow_accumulates_rounds_across_the_loop() {
    let before = 2;
    let mapped = map_think_result(
        sample_results(before)
            .into_iter()
            .find(|r| {
                matches!(
                    r,
                    crate::service::domain::runtime::types::ThinkLoopResult::ContextOverflow { .. }
                )
            })
            .expect("overflow sample"),
        before,
    );
    // 溢出信号是回环边，轮次必须累加 —— Budget 是唯一的进展护栏
    assert_eq!(mapped.signal.tag(), "overflow");
    assert!(
        mapped.rounds > before,
        "overflow must accumulate rounds, got {}",
        mapped.rounds
    );
}

/// 四种退出的轮次记账必须与老 `awaken` 逐位对齐
///
/// ⚠️ 这些数字**没有任何测试会替你看着**：写错一位不影响 Runners、不报错、
/// 用户也感知不到，只是 Trace 的 `rounds_used` 和兜底存档里的「做了多少轮」
/// 静默跑偏。所以它们只能靠这张表锁死。
#[test]
fn exit_mappings_keep_legacy_rounds_accounting() {
    let before = 2;

    // Final：老代码在此分支**不累加**（轮次信息只体现在策略心跳里）
    let final_ = map_think_result(
        sample_results(before)
            .into_iter()
            .find(|r| {
                matches!(
                    r,
                    crate::service::domain::runtime::types::ThinkLoopResult::Final { .. }
                )
            })
            .expect("final sample"),
        before,
    );
    assert_eq!(final_.rounds, before, "Final 不应改动累计轮次");

    // MaxRoundsExceeded：结果自带的就是累计值，**覆盖**而不是累加
    let exhausted = map_think_result(
        ThinkLoopResult::MaxRoundsExceeded {
            messages: Vec::new(),
            total_rounds: 7,
        },
        before,
    );
    assert_eq!(exhausted.rounds, 7, "轮次耗尽应直接采用自带的累计值");

    // Cancelled：被打断的那一段也算做过（对齐 `total_rounds + rounds`）
    let cancelled = map_think_result(
        ThinkLoopResult::Cancelled {
            messages: Vec::new(),
            total_rounds: 3,
        },
        before,
    );
    assert_eq!(cancelled.rounds, before + 3, "取消应把已跑轮次累加进去");
}

#[test]
fn every_exit_route_reaches_consolidate() {
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
    for exit in ["consolidate", "force_compact", "compact"] {
        assert!(
            reachable_from(exit).contains(&"consolidate".to_string()),
            "{exit} cannot reach consolidate"
        );
    }
}

/// 回到 `prepare` 的边只有压缩回环一条
#[test]
fn only_compaction_loops_back_to_prepare() {
    let bp = blueprint();
    let back: Vec<&str> = bp
        .edges
        .iter()
        .filter(|e| e.to == "prepare")
        .map(|e| e.from.as_str())
        .collect();
    assert_eq!(back, vec!["compact"]);
}
