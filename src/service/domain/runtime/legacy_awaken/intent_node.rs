//! 意图识别节点：包 `analyze_input_intent` 的场景化实现
//!
//! 失败降级为空 `IntentAnalysis` 继续走图（`types.rs` `IntentAnalysis` doc 的既有
//! 约定：不阻塞主流程），错误只留日志——意图识别是增强，不是闸门。
//! 下游对空产物的自愈：`retrieve` 对空 key_terms 短路、`prepare` 的意图区块
//! 空则不渲染，链路等价于「没有 Phase 1」的老行为。

use super::backend::IntentInput;
use super::context::AwakenDeps;
use super::nodes::{
    Artifacts, EXIT_CLARIFY, EXIT_DIRECT, PromptPack, SIGNAL_CLARIFY, SIGNAL_DIRECT,
    write_artifacts,
};
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
            // 直答分支还要补 Prompt / Artifacts 给收尾节点（见 run 内注释）
            writes: vec![
                ContextKey::Intent,
                ContextKey::Prompt,
                ContextKey::Artifacts,
            ],
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

        // 简单问题直答：图上是 `intent --direct--> consolidate`，跳过
        // retrieve / prepare / deliberate 三个节点，省掉一次完整 LLM 调用。
        if analysis.has_direct_answer() {
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "intent direct answer hit, skip retrieve/prepare/deliberate, agent_id={}",
                deps.agent.po.id
            );
            return short_circuit(
                bus,
                EXIT_DIRECT,
                SIGNAL_DIRECT,
                analysis.direct_answer.clone(),
                "[直答路径] 意图阶段判定为简单问题，跳过检索与装配直接作答。",
            );
        }

        // 信息不足需要澄清：图上 `intent --clarify--> consolidate`。
        // 缺的是用户输入而不是知识，往下走只会白跑一次 LLM。
        let questions: Vec<&str> = analysis
            .need_clarification
            .iter()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
            .collect();
        if !questions.is_empty() {
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "intent clarification hit, skip retrieve/prepare/deliberate, questions={}",
                questions.len()
            );
            return short_circuit(
                bus,
                EXIT_CLARIFY,
                SIGNAL_CLARIFY,
                build_clarification_reply(&questions),
                "[澄清路径] 意图阶段判定信息不足，跳过检索与装配直接向用户提问。",
            );
        }

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}

/// 意图阶段的短路退出：不检索、不装配、不执行，带着面向用户的文本直接收尾
///
/// 直答与澄清共用。收尾节点要求 Prompt + Artifacts，这里补一份最小占位
/// （Prompt 只用于 Trace 的 input，不会被任何节点再读）。
fn short_circuit(
    bus: &mut ThinkBus,
    exit: &str,
    signal: &str,
    reply: String,
    prompt_note: &str,
) -> Result<NodeOutcome> {
    let pack = PromptPack {
        prompt: prompt_note.to_string(),
        initial_messages: Vec::new(),
    };
    let pack_value = serde_json::to_value(&pack).map_err(|e| {
        common::error::err!(Internal, "encode short-circuit prompt pack failed: {e}")
    })?;
    bus.insert(ContextKey::Prompt, LoadDepth::Full, pack_value);
    write_artifacts(
        bus,
        &Artifacts {
            exit: exit.to_string(),
            content: reply,
            messages: Vec::new(),
        },
    )?;
    Ok(NodeOutcome::new(NodeSignal::Yield(signal.to_string())))
}

/// 把意图阶段给出的澄清点拼成一句面向用户的追问
///
/// 单条直接问，多条列成编号清单（比一段连写更容易逐条回答）。
/// 入参已过滤空串，因此至少一条。
fn build_clarification_reply(questions: &[&str]) -> String {
    if questions.len() == 1 {
        return format!("为了准确处理，我需要先确认一下：{}", questions[0]);
    }
    let mut reply = String::from("为了准确处理，我需要先确认几点：\n");
    for (i, q) in questions.iter().enumerate() {
        reply.push_str(&format!("{}. {}\n", i + 1, q));
    }
    reply.trim_end().to_string()
}
