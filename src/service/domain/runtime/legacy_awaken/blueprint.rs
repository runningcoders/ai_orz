//! `legacy_awaken` 蓝图：把老 awaitaken 的控制流写成拓扑
//!
//! ```text
//! prepare ──done──▶ deliberate ──done──▶ consolidate
//!                      │  │  └──cancelled────────┘
//!          ┌─done──┘   └──max_rounds──▶ force_compact ──done──┐
//!        compact ◀──overflow──┘                               │
//!          └──────────────────────────────────────────────────┘
//! ```
//!
//! 三条特殊边对应老链路的三个拐弯：
//! - `overflow` → 压缩后**回 `prepare` 重装配**（`compact → prepare` 是那条回环边）
//! - `max_rounds` → 压缩退出，产物就是给用户的回复
//! - `cancelled` → 直接进收尾（走兜底存档）

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

pub fn legacy_awaken_blueprint() -> Result<ThinkPathBlueprint> {
    let mut bp = ThinkPathBuilder::new("legacy_awaken")
        .entry("prepare")
        .node_with("prepare", PhaseId::Prepare, 0, "装配本轮 Prompt")
        .node_with("deliberate", PhaseId::Deliberate, 0, "执行一轮思考")
        .node_with("compact", PhaseId::Prepare, 0, "溢出压缩后重来")
        .node_with("force_compact", PhaseId::Prepare, 0, "轮次耗尽压缩退出")
        .node_with("consolidate", PhaseId::Consolidate, 0, "收尾落档")
        .edge("prepare", "deliberate", Some("done"))
        .edge("deliberate", "consolidate", Some("done"))
        .edge("deliberate", "compact", Some("overflow"))
        .edge("compact", "prepare", Some("done"))
        .edge("deliberate", "force_compact", Some("max_rounds"))
        .edge("force_compact", "consolidate", Some("done"))
        .edge("deliberate", "consolidate", Some("cancelled"))
        .build()?;

    // 只有「允许重复出现」的节点才显式放开预算，其余保持默认一次。
    // `compact` 比 `prepare` 少一次：第 N 次 prepare 才有第 N-1 次压缩。
    for node in bp.nodes.iter_mut() {
        node.budget.max_activations = Some(match node.id.as_str() {
            "prepare" | "deliberate" => MAX_LOOP_ROUNDS,
            "compact" => MAX_LOOP_ROUNDS.saturating_sub(1),
            _ => 1,
        });
    }
    Ok(bp)
}
