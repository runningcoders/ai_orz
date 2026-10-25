//! 检索节点（机械）· 骨架
//!
//! TODO(Task 4)：替换为真正实现——按意图关键词做知识混合检索
//! （`MemorySearch` Top-K）+ 思维方法技能召回（`category=thinking_method`），
//! 与 `deps.retrieval.queried_terms` 求差集做 visited 去重，
//! 产出完整 `RetrievalPack` 写入总线 `ContextKey::Knowledge` 分区。
//!
//! 骨架期行为：直通（Done）——不查库、不调模型，只写入空 `RetrievalPack`
//! 占位，保证「契约声明会写的分区真的有人写」，链路行为与旧拓扑等价。

use super::context::{AwakenDeps, RetrievalPack};
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_RETRIEVE: &str = "retrieve";

#[derive(Debug)]
pub struct RetrieveNode {
    deps: Arc<AwakenDeps>,
}

impl RetrieveNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for RetrieveNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_RETRIEVE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Prepare
    }

    fn purpose(&self) -> &str {
        "知识+方法技能检索（骨架直通，待 Task 4 替换）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            reads: vec![ContextKey::Intent],
            writes: vec![ContextKey::Knowledge],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        // TODO(Task 4)：bus[Intent] 关键词 → visited 差集 → 混合检索 + 技能召回
        // 快照后立刻放锁：MutexGuard 不跨 await（照 BusinessProvider 的先例）
        let queried = self
            .deps
            .retrieval
            .lock()
            .map(|s| s.queried_terms.len())
            .unwrap_or(0);
        crate::log_info!(
            &self.deps.ctx,
            "legacy_awaken",
            "retrieve node skeleton passthrough, agent_id={}, queried_terms={}",
            self.deps.agent.po.id,
            queried
        );

        let value = serde_json::to_value(RetrievalPack::default())
            .map_err(|e| common::error::err!(Internal, "encode retrieval pack failed: {e}"))?;
        bus.insert(ContextKey::Knowledge, LoadDepth::Full, value);
        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
