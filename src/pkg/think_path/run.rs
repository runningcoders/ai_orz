//! 运行实例与执行轨迹
//!
//! 蓝图 = 静态模板（可复用）；实例 = 独一无二的一次「思维」。
//! 实例持有**不可变蓝图快照**，因此蓝图升级后存量实例仍可按当时版本回放。
//!
//! 轨迹记的是 **occurrence 序列**而不是节点序列：
//! `Triage#1 → Prepare#1 → Deliberate#1 → Triage#2 → Consolidate#1`
//! 「小脑第二次介入」= `Triage#2`，定义可复用、出现是新的。

use super::blueprint::ThinkPathBlueprint;
use super::bus::ThinkBus;
use super::strategy::StrategyOverlay;
use super::types::{NodeId, NodeSignal, Stimulus};
use std::sync::Arc;

/// 一次运行：数据总线 + 轨迹 + 策略覆盖
pub struct ThinkRun {
    pub run_id: String,
    /// 不可变蓝图快照
    pub blueprint: Arc<ThinkPathBlueprint>,
    pub stimulus: Stimulus,
    pub bus: ThinkBus,
    pub trace: RunTrace,
    pub overlay: StrategyOverlay,
}

/// 节点「出现」：定义可复用，出现是新的
#[derive(Debug, Clone)]
pub struct NodeOccurrence {
    pub def: NodeId,
    pub seq: usize,
    pub step_index: usize,
    pub signal: Option<NodeSignal>,
}

/// 这一步是谁定的：可观测性的核心字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteVia {
    /// 合法候选唯一，零成本
    Unique,
    /// 多个候选，按确定性优先级选中
    Deterministic,
    /// 多个候选，由顾问（小脑/大脑）选出
    Advisor,
    /// 顾问不可用或越界，降级为确定性选择
    Fallback,
}

#[derive(Debug, Clone)]
pub struct RouteRecord {
    pub step: usize,
    pub candidates: Vec<NodeId>,
    pub chosen: NodeId,
    pub via: RouteVia,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct RunTrace {
    /// 已执行步数（1-based 计数）
    pub steps: usize,
    pub occurrences: Vec<NodeOccurrence>,
    pub routes: Vec<RouteRecord>,
    /// 顾问调用次数，受 `PathLimits::advisor_calls_budget` 约束
    pub advisor_calls: usize,
}

impl RunTrace {
    pub fn count(&self, id: &NodeId) -> usize {
        self.occurrences.iter().filter(|o| &o.def == id).count()
    }

    pub fn last_step_of(&self, id: &NodeId) -> Option<usize> {
        self.occurrences
            .iter()
            .rev()
            .find(|o| &o.def == id)
            .map(|o| o.step_index)
    }

    /// 路径指纹：实际走过的 occurrence 序列
    ///
    /// 聚合它能回答「最近 100 次 awaken 里有多少走了 `Triage#2`」——
    /// 动态性越强，这个统计越不是可选项。
    pub fn fingerprint(&self) -> String {
        self.occurrences
            .iter()
            .map(|o| format!("{}#{}", o.def, o.seq))
            .collect::<Vec<_>>()
            .join(">")
    }
}
