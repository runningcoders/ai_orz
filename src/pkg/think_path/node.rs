//! 节点定义：契约 + 准入条件 + 执行
//!
//! 关键约定：**节点不知道谁会激活它，也不知道自己后面是谁。**
//! 它只声明「我在什么条件下可被激活」（`ActivationRule`）。
//! 拓扑因此可改、可加节点而不动任何已有节点 —— 这是「可组合」的实质。
//!
//! 对比：如果把后继写在节点上（出边模型），改拓扑就得改节点代码，
//! 那只是把 `if/else` 换个地方写。

use super::bus::ThinkBus;
use super::strategy::StrategyPatch;
use super::types::{ContextKey, LoadDepth, NodeId, NodeSignal, PhaseId};
use async_trait::async_trait;
use common::error::Result;

/// 节点契约：声明式读写，是防止总线变成上帝对象的护栏
#[derive(Debug, Clone, Default)]
pub struct NodeContract {
    /// 前置依赖（引擎 `ensure_loaded` 保障；缺失按供给器 `on_failure` 处理）
    pub requires: Vec<(ContextKey, LoadDepth)>,
    pub reads: Vec<ContextKey>,
    /// 声明会写入的分区；未声明却写入在 debug 下 fail fast
    pub writes: Vec<ContextKey>,
}

/// 总线谓词：准入条件的原子判断
#[derive(Debug, Clone)]
pub enum BusPredicate {
    /// 该分区已加载
    KeyLoaded(ContextKey),
    /// 该分区已达指定深度
    KeyAtLeast(ContextKey, LoadDepth),
    /// 布尔字段断言：`path` 为 JSON Pointer（如 `/needs_deep_think`）
    FlagEq {
        key: ContextKey,
        path: String,
        expect: bool,
    },
}

// ⚠️ `BusPredicate` 是**声明**，不自带求值：
// 运行期判定统一走 `policy::BusPredicatePolicy`（投影成算子后由 `pkg::policy` 求值），
// 避免「枚举内一份判定逻辑 + 策略里一份」的双份真相。

/// 准入规则：挂在 `to` 上。空集合 = 不约束该项
#[derive(Debug, Clone, Default)]
pub struct ActivationRule {
    /// 允许的前驱节点
    pub after: Vec<NodeId>,
    /// 允许的前驱信号标签
    pub on: Vec<String>,
    /// 总线谓词，全部满足才可激活
    pub when: Vec<BusPredicate>,
    /// 冲突仲裁优先级（大者优先）
    pub priority: i32,
}

/// 激活预算：图 + 回环下的**终止性护栏**，不是配置项
#[derive(Debug, Clone)]
pub struct ActivationBudget {
    /// 本次 run 内最多激活次数
    pub max_activations: usize,
    /// 两次激活之间至少间隔的步数；0 = 不约束
    pub cooldown_steps: usize,
}

impl Default for ActivationBudget {
    /// ⚠️ 默认**只激活一次**
    ///
    /// 这是终止性的安全默认位：准入条件里 `after` 为空表示「不约束前驱」，
    /// 若再放开次数，任何节点都能被无限重入，图必然跑不完。
    /// 需要重复出现的节点（如 `Triage#2` 二次介入、压缩回环）必须**显式**放开预算。
    fn default() -> Self {
        Self {
            max_activations: 1,
            cooldown_steps: 0,
        }
    }
}

/// 节点产出：信号 + 策略提案
#[derive(Debug, Clone)]
pub struct NodeOutcome {
    pub signal: NodeSignal,
    /// 策略提案，由仲裁层校验后应用；节点不直接改策略
    pub patches: Vec<StrategyPatch>,
}

impl NodeOutcome {
    pub fn new(signal: NodeSignal) -> Self {
        Self {
            signal,
            patches: Vec::new(),
        }
    }

    pub fn with_patches(mut self, patches: Vec<StrategyPatch>) -> Self {
        self.patches = patches;
        self
    }
}

impl From<NodeSignal> for NodeOutcome {
    fn from(signal: NodeSignal) -> Self {
        Self::new(signal)
    }
}

/// 思维节点
#[async_trait]
pub trait ThinkNode: Send + Sync + std::fmt::Debug {
    fn id(&self) -> NodeId;

    fn phase(&self) -> PhaseId {
        PhaseId::Deliberate
    }

    /// 一句话用途：进 `RouterView` 供大小脑判断「该不该叫它」
    fn purpose(&self) -> &str {
        ""
    }

    fn contract(&self) -> NodeContract {
        NodeContract::default()
    }

    fn activation(&self) -> ActivationRule {
        ActivationRule::default()
    }

    fn budget(&self) -> ActivationBudget {
        ActivationBudget::default()
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome>;
}
