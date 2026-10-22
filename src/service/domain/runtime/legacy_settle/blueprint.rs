//! `legacy_settle` 蓝图：把「扫完所有未沉淀记忆」写成拓扑
//!
//! ```text
//! prepare ──ready──▶ settle ──next──▶ prepare
//!    │                  │
//!    └──empty──▶ finalize ◀──blocked──┘
//! ```
//!
//! 三条边对应老 `sleep_and_settle` 的三个拐弯：
//! - `next` → 回到 `prepare` 取下一批（只处理完一批不等于扫完队列）；
//! - `empty` → 队列取空，进收尾；
//! - `blocked` → 溢出 / 超轮次 / 取消：**结束本轮**，不标记本批（避免反复选中同一批）。
//!
//! ## 停车点
//!
//! 图里有环就必须有停车点：`prepare` / `settle` 各给 [`MAX_SETTLE_BATCHES`] 次预算。
//! 预算耗尽时引擎会因「没有合法候选」而正常结束 —— 该情形由 `runner` 的
//! `ensure_finalized` 转成显式错误，不会静默丢路径。

use crate::pkg::think_path::{PathLimits, PhaseId, ThinkPathBlueprint, ThinkPathBuilder};
use common::error::Result;

/// 一次沉淀 run 最多处理多少批
///
/// 真实场景几乎没有 Agent 会积压到 64 批（每批都吃满 40% 上下文）；
/// 撑到这个数基本说明预算计算或状态闭环出了 bug —— 此时停下来报错，
/// 比「跑了一夜还没跑完」更容易被发现。
pub(crate) const MAX_SETTLE_BATCHES: usize = 64;

pub fn legacy_settle_blueprint() -> Result<ThinkPathBlueprint> {
    let mut bp = ThinkPathBuilder::new("legacy_settle")
        .entry(super::nodes::NODE_PREPARE)
        .node_with(
            super::nodes::NODE_PREPARE,
            PhaseId::Prepare,
            0,
            "取一批未沉淀记忆：预算内累加，装配沉淀 Prompt",
        )
        .node_with(
            super::nodes::NODE_SETTLE,
            PhaseId::Deliberate,
            0,
            "执行一轮沉淀思考",
        )
        .node_with(
            super::nodes::NODE_FINALIZE,
            PhaseId::Consolidate,
            0,
            "收尾：聚合统计 + 循环完成事件",
        )
        .edge(
            super::nodes::NODE_PREPARE,
            super::nodes::NODE_SETTLE,
            Some(super::nodes::SIGNAL_READY),
        )
        .edge(
            super::nodes::NODE_SETTLE,
            super::nodes::NODE_PREPARE,
            Some(super::nodes::SIGNAL_NEXT),
        )
        .edge(
            super::nodes::NODE_PREPARE,
            super::nodes::NODE_FINALIZE,
            Some(super::nodes::SIGNAL_EMPTY),
        )
        .edge(
            super::nodes::NODE_SETTLE,
            super::nodes::NODE_FINALIZE,
            Some(super::nodes::SIGNAL_BLOCKED),
        )
        .limits(PathLimits {
            // 每次回环 = prepare + settle 两步；最后再多一次空 prepare 与一次 finalize
            max_steps: MAX_SETTLE_BATCHES * 2 + 2,
            timeout_secs: 0,
            advisor_calls_budget: 0,
        })
        .build()?;

    // 只有回环上的节点才显式放开预算，其余保持默认一次。
    // `prepare` 比 `settle` 多一次：最后那次 prepare 负责发现队列为空。
    for node in bp.nodes.iter_mut() {
        node.budget.max_activations = Some(match node.id.as_str() {
            super::nodes::NODE_PREPARE => MAX_SETTLE_BATCHES + 1,
            super::nodes::NODE_SETTLE => MAX_SETTLE_BATCHES,
            _ => 1,
        });
    }
    Ok(bp)
}
