//! 策略覆盖层（元认知的受控出口）
//!
//! 「思考节点根据总线调整策略」= 元认知。这里守一条硬边界：
//!
//! - 节点**只能提案改参数**（`StrategyPatch`），由仲裁层校验后落到 `StrategyOverlay`
//! - 节点**不能改规则集结构**——那等同运行时自修改代码，不可预测、不可测试、不可回滚
//!
//! 「大脑让小脑换个角度想」建模为：大脑产出 patch 降低小脑优先级 / 抬高检索深度，
//! 而不是大脑重写小脑的规则。这样每次「改主意」都是可审计、可回放的一条记录。

use super::types::{ContextKey, LoadDepth, NodeId};
use std::collections::{HashMap, HashSet};

/// 策略补丁：节点产出的提案
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyPatch {
    /// 提高/降低某节点的仲裁优先级
    RaisePriority { node: NodeId, delta: i32 },
    /// 抬高某分区的加载深度下限（如小脑要求记忆至少 Shallow）
    SetDepthFloor { key: ContextKey, min: LoadDepth },
    /// 本 run 内不再激活某节点
    Disable { node: NodeId, reason: String },
}

/// run 内可变的策略覆盖；仲裁时只读合并，不改节点定义本身
#[derive(Debug, Clone, Default)]
pub struct StrategyOverlay {
    pub priority_delta: HashMap<NodeId, i32>,
    pub depth_floor: HashMap<ContextKey, LoadDepth>,
    pub disabled: HashSet<NodeId>,
}

impl StrategyOverlay {
    pub fn apply(&mut self, patch: StrategyPatch) {
        match patch {
            StrategyPatch::RaisePriority { node, delta } => {
                *self.priority_delta.entry(node).or_insert(0) += delta;
            }
            StrategyPatch::SetDepthFloor { key, min } => {
                let slot = self.depth_floor.entry(key).or_insert(min);
                if *slot < min {
                    *slot = min;
                }
            }
            StrategyPatch::Disable { node, .. } => {
                self.disabled.insert(node);
            }
        }
    }

    pub fn effective_priority(&self, base: i32, node: &NodeId) -> i32 {
        base + self.priority_delta.get(node).copied().unwrap_or(0)
    }

    /// 该分区被抬高到的最低深度；`None` = 未覆盖，按节点声明走
    pub fn min_depth(&self, key: ContextKey) -> Option<LoadDepth> {
        self.depth_floor.get(&key).copied()
    }
}
