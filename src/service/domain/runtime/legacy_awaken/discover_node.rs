//! 新发现识别节点（判断型）· 骨架
//!
//! TODO(Task 6)：替换为真正实现——短聚焦无工具单轮模型调用，从思考轨迹提取
//! `DiscoveryOutcome`（new_key_terms + prior_thinking_summary），
//! 经 visited 差集去重后有新词 → `new_findings` 回 retrieve 再检索，
//! 无新词/预算尽/模型抖动 → `no_new_findings` 走压缩退出。
//!
//! 骨架期行为：恒走 `no_new_findings`（不调模型）——等价于旧拓扑里
//! `max_rounds → force_compact` 的直连，链路行为与旧拓扑等价。

use super::blueprint::SIGNAL_NO_NEW_FINDINGS;
use super::context::AwakenDeps;
use crate::pkg::think_path::{
    ContextKey, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus, ThinkNode,
};
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_DISCOVER: &str = "discover";

#[derive(Debug)]
pub struct DiscoverNode {
    deps: Arc<AwakenDeps>,
}

impl DiscoverNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for DiscoverNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_DISCOVER)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Review
    }

    fn purpose(&self) -> &str {
        "新发现识别：提取新线索或退出（骨架恒退出，待 Task 6 替换）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            reads: vec![
                ContextKey::Artifacts,
                ContextKey::Knowledge,
                ContextKey::Intent,
            ],
            writes: vec![ContextKey::Knowledge],
        }
    }

    async fn run(&self, _bus: &mut ThinkBus) -> Result<NodeOutcome> {
        // TODO(Task 6)：短聚焦 prompt → backend.discover → 宽容 JSON 解析 → 差集判定
        // 快照后立刻放锁：MutexGuard 不跨 await（照 BusinessProvider 的先例）
        let rounds = self
            .deps
            .retrieval
            .lock()
            .map(|s| s.discovery_rounds)
            .unwrap_or(0);
        crate::log_info!(
            &self.deps.ctx,
            "legacy_awaken",
            "discover node skeleton: no new findings, agent_id={}, discovery_rounds={}",
            self.deps.agent.po.id,
            rounds
        );
        Ok(NodeOutcome::new(NodeSignal::Yield(
            SIGNAL_NO_NEW_FINDINGS.to_string(),
        )))
    }
}
