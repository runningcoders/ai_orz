//! 蓝图：可序列化、可版本化、**不含任何行为**
//!
//! 序列化的是**拓扑与配置**，不是行为 —— `Box<dyn ThinkNode>` 本来就不可序列化，
//! 运行时按 id 从节点池解析实现。
//!
//! `EdgeSpec` 是 builder 层的**语法糖**：引擎构造时把它编译进目标节点的准入条件
//! （`from → to` 翻转为 `to.activation.after/on`）。这样上层仍可静态分析
//! （可达性 / 孤儿节点 / 环检测），底层统一跑准入仲裁 —— **静态是动态的特例**。

use super::types::{NodeId, PhaseId};
use common::error::{Result, bail_err};

// 约定：本模块的蓝图/引擎结构性错误统一用 `Internal`，不新增 ErrorCode 变体，
// 避免连带改动 HTTP 错误码映射表。
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetSpec {
    /// `None` = 沿用节点实现自带的 `ActivationBudget::default()`
    pub max_activations: Option<usize>,
    pub cooldown_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
    pub id: String,
    pub phase: String,
    pub priority: i32,
    #[serde(default)]
    pub budget: BudgetSpec,
    #[serde(default)]
    pub purpose: String,
    /// 节点实现参数（序列化的是配置，不是行为）
    #[serde(default)]
    pub params: serde_json::Value,
}

/// 静态边：编译为目标节点准入条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeSpec {
    pub from: String,
    pub to: String,
    /// 匹配的前驱信号标签；`None` = 任意
    pub on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathLimits {
    /// 硬护栏：图 + 回环下的死循环封顶
    pub max_steps: usize,
    /// 0 = 不限
    pub timeout_secs: u64,
    /// 路由顾问（LLM）调用次数上限，成本护栏；0 = 不启用顾问
    pub advisor_calls_budget: usize,
}

impl Default for PathLimits {
    fn default() -> Self {
        Self {
            max_steps: 64,
            timeout_secs: 0,
            advisor_calls_budget: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkPathBlueprint {
    pub id: String,
    pub version: u32,
    pub entry: String,
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
    #[serde(default)]
    pub limits: PathLimits,
}

pub struct ThinkPathBuilder {
    blueprint: ThinkPathBlueprint,
}

impl ThinkPathBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            blueprint: ThinkPathBlueprint {
                id: id.into(),
                version: 1,
                entry: String::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
                limits: PathLimits::default(),
            },
        }
    }

    pub fn version(mut self, version: u32) -> Self {
        self.blueprint.version = version;
        self
    }

    pub fn entry(mut self, id: impl Into<String>) -> Self {
        self.blueprint.entry = id.into();
        self
    }

    pub fn node(mut self, id: impl Into<String>, phase: PhaseId) -> Self {
        self.blueprint.nodes.push(NodeSpec {
            id: id.into(),
            phase: phase.as_str().to_string(),
            priority: 0,
            budget: BudgetSpec::default(),
            purpose: String::new(),
            params: serde_json::Value::Null,
        });
        self
    }

    pub fn node_with(
        mut self,
        id: impl Into<String>,
        phase: PhaseId,
        priority: i32,
        purpose: impl Into<String>,
    ) -> Self {
        self.blueprint.nodes.push(NodeSpec {
            id: id.into(),
            phase: phase.as_str().to_string(),
            priority,
            budget: BudgetSpec::default(),
            purpose: purpose.into(),
            params: serde_json::Value::Null,
        });
        self
    }

    /// 静态边：`on` 为 `None` 表示任意信号
    pub fn edge(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        on: Option<&str>,
    ) -> Self {
        self.blueprint.edges.push(EdgeSpec {
            from: from.into(),
            to: to.into(),
            on: on.map(str::to_string),
        });
        self
    }

    pub fn limits(mut self, limits: PathLimits) -> Self {
        self.blueprint.limits = limits;
        self
    }

    pub fn build(self) -> Result<ThinkPathBlueprint> {
        let bp = self.blueprint;
        if bp.id.is_empty() {
            bail_err!(Internal, "think path blueprint id must not be empty");
        }
        if bp.entry.is_empty() {
            bail_err!(Internal, "think path blueprint entry must be set");
        }
        let known: HashSet<&str> = bp.nodes.iter().map(|n| n.id.as_str()).collect();
        if !known.contains(bp.entry.as_str()) {
            bail_err!(
                Internal,
                "think path entry node is not declared: path={} entry={}",
                bp.id,
                bp.entry
            );
        }
        for edge in &bp.edges {
            for side in [edge.from.as_str(), edge.to.as_str()] {
                if !known.contains(side) {
                    bail_err!(
                        Internal,
                        "think path edge references an undeclared node: path={} node={}",
                        bp.id,
                        side
                    );
                }
            }
        }
        Ok(bp)
    }
}

/// 蓝图 id 转 `NodeId` 的小工具，供引擎/测试复用
pub fn to_node_id(spec: &NodeSpec) -> NodeId {
    NodeId::new(spec.id.clone())
}
