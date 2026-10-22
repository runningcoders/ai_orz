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

/// 压缩/思考回环的次数上限
///
/// 旧实现里是隐式常量（`COMPACT_MAX_ROUNDS`），这里是**显式写进蓝图的预算**——
/// 它是终止性护栏，不是配置：图 + 回环必须靠它停下。
const COMPACT_MAX_ROUNDS: usize = 3;

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

    // 只有「允许重复出现」的节点才显式放开预算，其余保持默认一次
    for node in bp.nodes.iter_mut() {
        node.budget.max_activations = Some(match node.id.as_str() {
            "prepare" | "deliberate" => COMPACT_MAX_ROUNDS + 1,
            "compact" => COMPACT_MAX_ROUNDS,
            _ => 1,
        });
    }
    Ok(bp)
}
