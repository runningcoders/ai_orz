//! 意图识别节点：包 `analyze_input_intent` 的场景化实现
//!
//! 失败降级为空 `IntentAnalysis` 继续走图（`types.rs` `IntentAnalysis` doc 的既有
//! 约定：不阻塞主流程），错误只留日志——意图识别是增强，不是闸门。
//! 下游对空产物的自愈：`retrieve` 对空 key_terms 短路、`prepare` 的意图区块
//! 空则不渲染，链路等价于「没有 Phase 1」的老行为。

use super::backend::IntentInput;
use super::context::AwakenDeps;
use crate::models::events::AgentLoopEvent;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::types::IntentAnalysis;
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;
use std::time::SystemTime;

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
        "意图识别：产出结构化 IntentAnalysis（失败降级空结构）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            reads: vec![],
            writes: vec![ContextKey::Intent],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        // 子 trace 约定：intent-{主 trace_id}，监控里与主唤醒循环可区分
        let trace_id = format!("intent-{}", deps.trace_id);
        let started = SystemTime::now();
        let _ = crate::pkg::aop::publish(
            &deps.ctx,
            AgentLoopEvent::started(&deps.agent.po.id, &trace_id, "intent-analyze", None),
        )
        .await;

        // 失败降级：空 IntentAnalysis，主流程照常（retrieve 对空词自行短路）。
        // 事件口径与 awakening::analyze_input_intent 对齐：失败也发 finished 事件
        // （status=failed），监控才能追踪 Phase 1 的失败率与耗时。
        let (analysis, status) = match deps
            .backend
            .analyze_intent(
                deps,
                IntentInput {
                    trace_id: trace_id.clone(),
                },
            )
            .await
        {
            Ok(ia) => (ia, "success".to_string()),
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "intent analyze degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                (IntentAnalysis::default(), format!("failed: {e}"))
            }
        };

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "intent node done, agent_id={}, intent_type={}, confidence={}, key_terms={}",
            deps.agent.po.id,
            analysis.intent_type,
            analysis.confidence,
            analysis.key_terms.len()
        );

        let duration_ms = started.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
        let _ = crate::pkg::aop::publish(
            &deps.ctx,
            AgentLoopEvent::finished(
                &deps.agent.po.id,
                &trace_id,
                "intent-analyze",
                &status,
                duration_ms,
                None,
            ),
        )
        .await;

        let value = serde_json::to_value(&analysis)
            .map_err(|e| common::error::err!(Internal, "encode intent analysis failed: {e}"))?;
        bus.insert(ContextKey::Intent, LoadDepth::Full, value);
        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
