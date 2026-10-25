//! 意图识别节点 · 骨架
//!
//! TODO(Task 3)：替换为真正实现——包 `analyze_input_intent_inner` 跑 IntentAnalyze
//! 场景小循环，产出 `IntentAnalysis` 写入总线 `ContextKey::Intent` 分区；
//! 失败降级为空 IntentAnalysis 继续走图（意图识别是增强，不是闸门）。
//!
//! 骨架期行为：**直通**（Done）——不调模型、不产出意图产物。蓝图先行落地时
//! 既有链路（prepare → deliberate → …）照常可跑，集成测试保持全绿；
//! 真正的 `run` 由后续任务整体替换本文件。

use super::context::AwakenDeps;
use crate::pkg::think_path::{
    ContextKey, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus, ThinkNode,
};
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_INTENT: &str = "intent";

#[derive(Debug)]
pub struct IntentNode {
    deps: Arc<AwakenDeps>,
}

impl IntentNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for IntentNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_INTENT)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Triage
    }

    fn purpose(&self) -> &str {
        "意图识别：产出结构化 IntentAnalysis（骨架直通，待 Task 3 替换）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            reads: vec![],
            writes: vec![ContextKey::Intent],
        }
    }

    async fn run(&self, _bus: &mut ThinkBus) -> Result<NodeOutcome> {
        // TODO(Task 3)：backend.analyze_intent + 失败降级空 IntentAnalysis
        crate::log_info!(
            &self.deps.ctx,
            "legacy_awaken",
            "intent node skeleton passthrough, agent_id={}",
            self.deps.agent.po.id
        );
        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
