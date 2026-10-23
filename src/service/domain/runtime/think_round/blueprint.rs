//! 轮级环图拓扑
//!
//! ```text
//! think --tool_calls--> dispatch --done--> evaluate
//!   ^                      （工具执行）        │
//!   └──────────── continue ───────────────────┘
//!   │
//!   └── final ───────────────────────────> evaluate --exit--> 结束
//! ```
//!
//! - `think`：调用模型一轮（含 LLM 失败重试）
//! - `dispatch`：执行本轮工具调用、记录进度快照、发布事件、注入疲劳提示
//! - `evaluate`：统一策略裁决（final / tool_calls 两套指标），决定 continue 还是 exit
//!
//! `exit` 与 `cancelled` 都没有出边 ⇒ 引擎无合法候选，run 自然终止。
//!
//! ⚠️ 准入规则的信号约束是**节点级集合**（`after` / `on` 各自 OR，再整体 AND），
//! 不是逐边判定。因此 `evaluate` 的 `on` 集合必须同时容纳两个前驱的信号：
//! `think` 的 `final` 与 `dispatch` 的 `done`——只写 `final` 会让 dispatch 之后
//! 的 evaluate 永远不合法。

use crate::pkg::think_path::types::SIGNAL_DONE;
use crate::pkg::think_path::{PathLimits, PhaseId, ThinkPathBlueprint, ThinkPathBuilder};
use common::error::Result;

pub(crate) const NODE_THINK: &str = "think";
pub(crate) const NODE_DISPATCH: &str = "dispatch";
pub(crate) const NODE_EVALUATE: &str = "evaluate";
pub(crate) const NODE_REFLECT: &str = "reflect";

pub(crate) const SIGNAL_TOOL_CALLS: &str = "tool_calls";
pub(crate) const SIGNAL_FINAL: &str = "final";
pub(crate) const SIGNAL_CONTINUE: &str = "continue";
pub(crate) const SIGNAL_EXIT: &str = "exit";
pub(crate) const SIGNAL_CANCELLED: &str = "cancelled";
/// evaluate 判定「本轮工具反复调用、值得换方向」→ 进入 reflect
pub(crate) const SIGNAL_REFLECT: &str = "reflect";
/// reflect 注入换方向提示后回到下一轮 think
pub(crate) const SIGNAL_REDIRECT: &str = "redirect";

/// 构造轮级环图蓝图
///
/// 默认每轮最多 3 步（think / dispatch / evaluate），最后一轮最多 2 步
/// （think / evaluate）。启用 reflect 时每轮多一步（evaluate / reflect），
/// 上限同步放宽到 `4 * rounds + 2`。步数上限是回环下的死循环硬护栏。
pub(crate) fn round_blueprint(
    available_rounds: usize,
    reflect_enabled: bool,
) -> Result<ThinkPathBlueprint> {
    let mut builder = ThinkPathBuilder::new("think_round")
        .entry(NODE_THINK)
        .node_with(NODE_THINK, PhaseId::Deliberate, 0, "调用模型进行一轮思考")
        .node_with(NODE_DISPATCH, PhaseId::Review, 0, "执行本轮工具调用")
        .node_with(NODE_EVALUATE, PhaseId::Review, 0, "策略裁决：继续还是退出")
        .edge(NODE_THINK, NODE_DISPATCH, Some(SIGNAL_TOOL_CALLS))
        .edge(NODE_THINK, NODE_EVALUATE, Some(SIGNAL_FINAL))
        .edge(NODE_DISPATCH, NODE_EVALUATE, Some(SIGNAL_DONE))
        .edge(NODE_EVALUATE, NODE_THINK, Some(SIGNAL_CONTINUE));

    // 自省分支：同一工具反复调用时，evaluate 改发 reflect 信号，
    // reflect 注入「换方向」提示后再以 redirect 信号回到 think。
    // 关闭时不声明 reflect 节点，拓扑与老循环完全一致。
    if reflect_enabled {
        builder = builder
            .node_with(NODE_REFLECT, PhaseId::Review, 0, "自省：注入换方向提示")
            .edge(NODE_EVALUATE, NODE_REFLECT, Some(SIGNAL_REFLECT))
            .edge(NODE_REFLECT, NODE_THINK, Some(SIGNAL_REDIRECT));
    }

    let steps_per_round = if reflect_enabled { 4 } else { 3 };
    builder
        .limits(PathLimits {
            max_steps: available_rounds
                .saturating_mul(steps_per_round)
                .saturating_add(2),
            timeout_secs: 0,
            advisor_calls_budget: 0,
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blueprint_wires_round_loop() {
        let bp = round_blueprint(4, false).expect("blueprint");
        assert_eq!(bp.entry, NODE_THINK);
        assert_eq!(bp.nodes.len(), 3);
        assert_eq!(bp.limits.max_steps, 14);

        let has = |from: &str, to: &str, on: Option<&str>| {
            bp.edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.on.as_deref() == on)
        };
        assert!(has(NODE_THINK, NODE_DISPATCH, Some(SIGNAL_TOOL_CALLS)));
        assert!(has(NODE_THINK, NODE_EVALUATE, Some(SIGNAL_FINAL)));
        assert!(has(NODE_DISPATCH, NODE_EVALUATE, Some(SIGNAL_DONE)));
        assert!(has(NODE_EVALUATE, NODE_THINK, Some(SIGNAL_CONTINUE)));
        // 关闭 reflect 时不声明该节点，亦无分叉边
        assert!(!bp.nodes.iter().any(|n| n.id == NODE_REFLECT));
        assert!(
            !bp.edges
                .iter()
                .any(|e| e.from == NODE_EVALUATE && e.on.as_deref() == Some(SIGNAL_REFLECT))
        );
        // exit / cancelled 不设出边：无合法候选即自然终止
        assert!(
            !bp.edges
                .iter()
                .any(|e| e.from == NODE_EVALUATE && e.on.as_deref() == Some(SIGNAL_EXIT))
        );
    }

    #[test]
    fn blueprint_adds_reflect_branch_when_enabled() {
        let bp = round_blueprint(4, true).expect("blueprint");
        assert_eq!(bp.nodes.len(), 4);
        assert_eq!(bp.limits.max_steps, 18);

        let has = |from: &str, to: &str, on: Option<&str>| {
            bp.edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.on.as_deref() == on)
        };
        assert!(has(NODE_EVALUATE, NODE_REFLECT, Some(SIGNAL_REFLECT)));
        assert!(has(NODE_REFLECT, NODE_THINK, Some(SIGNAL_REDIRECT)));
    }

    #[test]
    fn single_round_has_enough_steps_for_tool_loop() {
        // 1 轮最坏情况：think + dispatch + evaluate = 3 步，上限 5 步
        assert_eq!(
            round_blueprint(1, false)
                .expect("blueprint")
                .limits
                .max_steps,
            5
        );
        // 启用 reflect：think + dispatch + evaluate + reflect = 4 步，上限 6 步
        assert_eq!(
            round_blueprint(1, true)
                .expect("blueprint")
                .limits
                .max_steps,
            6
        );
    }

    /// 信号准入是节点级集合：`evaluate` 必须能接住 dispatch 的 `done`
    #[tokio::test]
    async fn round_loop_runs_think_dispatch_evaluate() {
        use crate::pkg::think_path::node::ThinkNode;
        use crate::pkg::think_path::testkit::ProbeNode;
        use crate::pkg::think_path::{Arbiter, NodeSignal, ProviderSet, Stimulus, ThinkPathEngine};

        let nodes: Vec<std::sync::Arc<dyn ThinkNode>> = vec![
            std::sync::Arc::new(ProbeNode::new(
                NODE_THINK,
                NodeSignal::Yield(SIGNAL_TOOL_CALLS.to_string()),
            )),
            std::sync::Arc::new(ProbeNode::new(NODE_DISPATCH, NodeSignal::Done)),
            std::sync::Arc::new(ProbeNode::new(
                NODE_EVALUATE,
                NodeSignal::Yield(SIGNAL_CONTINUE.to_string()),
            )),
        ];
        let engine = ThinkPathEngine::new(
            round_blueprint(1, false).expect("blueprint"),
            nodes,
            Arbiter::deterministic(),
            ProviderSet::new(),
        )
        .expect("engine");

        let run = engine
            .run("round_loop", Stimulus::default())
            .await
            .expect("run");
        let visited: Vec<&str> = run
            .trace
            .occurrences
            .iter()
            .map(|o| o.def.as_str())
            .collect();
        // 预算耗尽后无合法候选，run 自然终止（对应真实节点的轮次上限）
        assert_eq!(visited, vec![NODE_THINK, NODE_DISPATCH, NODE_EVALUATE]);
    }

    /// 开启 reflect 后，evaluate 的 reflect 信号必须真正进入自省节点
    #[tokio::test]
    async fn reflect_branch_is_reachable() {
        use crate::pkg::think_path::node::ThinkNode;
        use crate::pkg::think_path::testkit::ProbeNode;
        use crate::pkg::think_path::{Arbiter, NodeSignal, ProviderSet, Stimulus, ThinkPathEngine};

        let nodes: Vec<std::sync::Arc<dyn ThinkNode>> = vec![
            std::sync::Arc::new(ProbeNode::new(
                NODE_THINK,
                NodeSignal::Yield(SIGNAL_TOOL_CALLS.to_string()),
            )),
            std::sync::Arc::new(ProbeNode::new(NODE_DISPATCH, NodeSignal::Done)),
            std::sync::Arc::new(ProbeNode::new(
                NODE_EVALUATE,
                NodeSignal::Yield(SIGNAL_REFLECT.to_string()),
            )),
            std::sync::Arc::new(ProbeNode::new(
                NODE_REFLECT,
                NodeSignal::Yield(SIGNAL_REDIRECT.to_string()),
            )),
        ];
        let engine = ThinkPathEngine::new(
            round_blueprint(1, true).expect("blueprint"),
            nodes,
            Arbiter::deterministic(),
            ProviderSet::new(),
        )
        .expect("engine");

        let run = engine
            .run("round_reflect", Stimulus::default())
            .await
            .expect("run");
        let visited: Vec<&str> = run
            .trace
            .occurrences
            .iter()
            .map(|o| o.def.as_str())
            .collect();
        // 轮次预算 = 1：自省后 think 不再被准入，run 自然终止
        assert_eq!(
            visited,
            vec![NODE_THINK, NODE_DISPATCH, NODE_EVALUATE, NODE_REFLECT]
        );
    }
}
