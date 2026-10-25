//! `legacy_awaken` 蓝图：把老 awaitaken 的控制流写成完整思考图
//!
//! ```text
//! intent ──done──▶ retrieve ──done──▶ prepare ──done──▶ deliberate
//!                      ▲                                 ├─done──────▶ consolidate
//!                      │                                 ├─cancelled─▶ consolidate
//!                      │                                 └─overflow──▶ compact ──done──▶ prepare
//!                      └────────── new_findings ── discover ◀── max_rounds
//!                                                     └─no_new_findings─▶ force_compact ──done──▶ consolidate
//! ```
//!
//! 主链 =「意图识别 → 检索 → 装配 → 执行」，`deliberate` 之后按退出的不同分岔。
//! 三条循环/退出边对应思考图的三种「再来一次或收手」：
//! - `overflow` → **压缩回环**：`compact → prepare` 重装配后再思考（既有语义）
//! - `max_rounds` → **新发现回环**：`discover` 从思考轨迹里判断是否出现值得
//!   再检索的新线索——有 → `discover → retrieve` 重新检索装配（new_findings）；
//!   无 → `discover → force_compact` 压缩退出（no_new_findings）
//! - `cancelled` → 直接进收尾（走兜底存档）
//!
//! 骨架阶段（intent/retrieve/discover 的真正实现落地前）：intent/retrieve 直通，
//! discover 恒走 no_new_findings——图形状先行，链路行为与旧拓扑等价。

use super::nodes::{SIGNAL_CANCELLED, SIGNAL_MAX_ROUNDS, SIGNAL_OVERFLOW};
use crate::pkg::think_path::types::SIGNAL_DONE;
use crate::pkg::think_path::{PhaseId, ThinkPathBlueprint, ThinkPathBuilder};
use common::error::Result;

/// 「溢出 → 压缩 → 重装配」回环的次数上限
///
/// ⚠️ 与 `compaction::COMPACT_MAX_ROUNDS` 不是一回事：那是**单次压缩内部**的
/// 思考轮次预算，这是整条唤醒链能重来几次。
///
/// ⚠️ 这是**新引入的显式护栏**。老 `awaken` 的同一段循环没有任何次数限制，
/// 只靠 `total_rounds >= max_rounds` 间接收敛；而图里有环就必须有停车点，
/// 否则 `prepare → deliberate → compact → prepare` 永远可以继续排。
///
/// 8 是刻意放宽的值：真实场景溢出重来通常 1~3 次（每次都要先跑满一轮思考
/// 再压一次），撑到 8 次基本已经是失控。万一真被耗尽，`runner` 会因为「没走到
/// `consolidate`」而**报错**（而不是静默结束）——停车点收紧了，但不再是静默的。
pub const MAX_LOOP_ROUNDS: usize = 8;

/// 新发现回环：discover 判定有值得检索的新线索，回 `retrieve` 再检索
pub(crate) const SIGNAL_NEW_FINDINGS: &str = "new_findings";
/// 无新线索：不走回环，经 `force_compact` 压缩退出收尾
pub(crate) const SIGNAL_NO_NEW_FINDINGS: &str = "no_new_findings";

pub fn legacy_awaken_blueprint() -> Result<ThinkPathBlueprint> {
    let mut bp = ThinkPathBuilder::new("legacy_awaken")
        .entry("intent")
        .node_with(
            "intent",
            PhaseId::Triage,
            0,
            "意图识别：产出结构化 IntentAnalysis",
        )
        .node_with(
            "retrieve",
            PhaseId::Prepare,
            0,
            "知识+方法技能检索（机械，Top-K）",
        )
        .node_with("prepare", PhaseId::Prepare, 0, "装配本轮 Prompt")
        .node_with("deliberate", PhaseId::Deliberate, 0, "执行一轮思考")
        .node_with("compact", PhaseId::Prepare, 0, "溢出压缩后重来")
        .node_with("force_compact", PhaseId::Prepare, 0, "轮次耗尽压缩退出")
        .node_with(
            "discover",
            PhaseId::Review,
            0,
            "新发现识别：提取新线索或退出",
        )
        .node_with("consolidate", PhaseId::Consolidate, 0, "收尾落档")
        .edge("intent", "retrieve", Some(SIGNAL_DONE))
        .edge("retrieve", "prepare", Some(SIGNAL_DONE))
        .edge("prepare", "deliberate", Some(SIGNAL_DONE))
        .edge("deliberate", "consolidate", Some(SIGNAL_DONE))
        .edge("deliberate", "compact", Some(SIGNAL_OVERFLOW))
        .edge("compact", "prepare", Some(SIGNAL_DONE))
        .edge("deliberate", "discover", Some(SIGNAL_MAX_ROUNDS))
        .edge("discover", "retrieve", Some(SIGNAL_NEW_FINDINGS))
        .edge("discover", "force_compact", Some(SIGNAL_NO_NEW_FINDINGS))
        .edge("force_compact", "consolidate", Some(SIGNAL_DONE))
        .edge("deliberate", "consolidate", Some(SIGNAL_CANCELLED))
        .build()?;

    // 只有「允许重复出现」的节点才显式放开预算，其余保持默认一次。
    // `compact` 比 `prepare` 少一次：第 N 次 prepare 才有第 N-1 次压缩；
    // `retrieve` / `discover` 是新发现回环的两个常客：3 次检索、2 次再识别。
    for node in bp.nodes.iter_mut() {
        node.budget.max_activations = Some(match node.id.as_str() {
            "prepare" | "deliberate" => MAX_LOOP_ROUNDS,
            "compact" => MAX_LOOP_ROUNDS.saturating_sub(1),
            "retrieve" => 3,
            "discover" => 2,
            _ => 1,
        });
    }
    Ok(bp)
}
