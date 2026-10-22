//! `legacy_settle` 的三个节点
//!
//! ```text
//! prepare ──ready──▶ settle ──next──▶ prepare
//!    │                  │
//!    └──empty──▶ finalize ◀──blocked──┘
//! ```
//!
//! 与老 `sleep_and_settle` 的对应关系：
//! - `prepare` = 取一批待沉淀记忆 + 装配沉淀 Prompt + 建批次 Trace / 运行时
//! - `settle` = 跑一轮沉淀思考 + 写 Trace + （仅 `Final`）标记 Settled
//! - `finalize` = 整次 run 的统计事件 + 循环完成事件
//!
//! ⚠️ **只有 `Final` 才标记 Settled**：溢出 / 超轮次 / 取消的批次都是无效沉淀，
//! 标记它等于把没整理进图谱的记忆永久丢掉。这几条退出路径直接结束本轮，
//! 记忆保持 Active 交给下一次沉淀（消费者重投幂等）。

use super::context::{PendingBatch, SettleDeps};
use super::pending::build_pending_memories_summary;
use crate::models::events::AgentLoopEvent;
use crate::models::memory::MemoryTrace;
use crate::pkg::stats::AgentAwakeEvent;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::awakening::{
    init_think_runtime_and_policy, mount_ontology_lexicon,
};
use crate::service::domain::runtime::legacy_awaken::context::read_bus;
use crate::service::domain::runtime::legacy_awaken::nodes::{
    EXIT_CANCELLED, EXIT_OVERFLOW, PromptPack, WorkspacePaths,
};
use crate::service::domain::runtime::types::ThinkLoopResult;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeMemory};
use async_trait::async_trait;
use common::enums::{MemoryRole, ThinkingScene};
use common::error::Result;
use std::sync::Arc;

/// 收尾节点的 id（与 `runner::ensure_finalized` 共用同一真相）
pub(crate) const NODE_FINALIZE: &str = "finalize";
pub(crate) const NODE_PREPARE: &str = "prepare";
pub(crate) const NODE_SETTLE: &str = "settle";

/// 节点出口信号
pub(crate) const SIGNAL_READY: &str = "ready";
pub(crate) const SIGNAL_NEXT: &str = "next";
pub(crate) const SIGNAL_EMPTY: &str = "empty";
pub(crate) const SIGNAL_BLOCKED: &str = "blocked";

/// 取总数上限（防 `MutexGuard` 跨 await）
fn lock_aggregate(deps: &SettleDeps) -> super::context::SettleAggregate {
    deps.aggregate.lock().map(|g| g.clone()).unwrap_or_default()
}

// ==================== Prepare：取一批 + 装配 Prompt ====================

/// 装配节点：查一批未沉淀记忆，产出本批的 Prompt / Trace / 思考运行时
#[derive(Debug)]
pub struct PrepareNode {
    deps: Arc<SettleDeps>,
}

impl PrepareNode {
    pub fn new(deps: Arc<SettleDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for PrepareNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_PREPARE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Prepare
    }

    fn purpose(&self) -> &str {
        "取一批未沉淀记忆：预算内累加，装配沉淀 Prompt"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![
                (ContextKey::Runtime, LoadDepth::Shallow),
                (ContextKey::Skills, LoadDepth::Shallow),
                (ContextKey::Tools, LoadDepth::Shallow),
            ],
            reads: vec![ContextKey::Runtime, ContextKey::Skills, ContextKey::Tools],
            writes: vec![ContextKey::Prompt],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let ctx = &deps.ctx;
        let agent_id = deps.agent.po.id.clone();

        crate::log_info!(
            ctx,
            "legacy_settle",
            "prepare node start, agent_id={}, budget_chars={}, item_cap={}",
            agent_id,
            deps.budget_chars,
            deps.item_cap()
        );

        // 取一批：无候选即本轮沉淀结束
        let Some((summary, ids, _truncated)) = build_pending_memories_summary(
            ctx,
            deps.domain.memory(),
            &agent_id,
            deps.budget_chars,
            deps.item_cap(),
        )
        .await?
        else {
            crate::log_info!(
                ctx,
                "legacy_settle",
                "prepare node: no pending memories, agent_id={}",
                agent_id
            );
            return Ok(NodeOutcome::new(NodeSignal::Yield(SIGNAL_EMPTY.into())));
        };

        // 本批独立的 Trace / 思考运行时（批与批之间互不污染取消信号与轮次）
        let trace = MemoryTrace::new(
            agent_id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(),
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();
        let (think_runtime, policy) =
            init_think_runtime_and_policy(&deps.agent, ThinkingScene::Settle, &trace_id);

        // 首个批次成为本次 run 的锚点：AOP started 只发一次
        let is_first = {
            let mut anchor = match deps.run_anchor.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if anchor.is_none() {
                *anchor = Some(trace_id.clone());
                true
            } else {
                false
            }
        };
        if is_first {
            let _ = crate::pkg::aop::publish(
                ctx,
                AgentLoopEvent::started(&agent_id, &trace_id, "settle", None),
            )
            .await;
        }

        // 装配沉淀 Prompt（与老 `sleep_and_settle` 同口径）
        let ws = read_bus::<WorkspacePaths>(bus, ContextKey::Runtime)?;
        let skills = read_bus::<Vec<crate::models::skill::SkillPo>>(bus, ContextKey::Skills)?;
        let settled_reference = deps
            .domain
            .query_settled_reference(ctx.clone(), &agent_id)
            .await;

        let mut builder = deps.domain.prompt_builder(&deps.agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(&deps.agent);
        builder.skills(&skills);
        builder.workspace_context(
            ws.default,
            ws.user_home,
            ws.user_shared,
            ws.user_agent,
            ws.agent,
            ws.project,
        );
        mount_ontology_lexicon(ctx, builder.as_mut()).await;
        builder.settled_reference(&settled_reference);

        let pack = PromptPack {
            prompt: builder.build_sleep_prompt(&summary, &deps.trace_ids),
            initial_messages: builder.build_sleep_initial_messages(&summary, &deps.trace_ids),
        };
        let value = serde_json::to_value(&pack)
            .map_err(|e| common::error::err!(Internal, "encode prompt pack failed: {e}"))?;
        bus.insert(ContextKey::Prompt, LoadDepth::Full, value);

        // 批次写入共享槽位，交给 settle 节点消费
        {
            let mut slot = match deps.batch.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            *slot = Some(PendingBatch {
                trace,
                ids: ids.clone(),
                think_runtime,
                policy,
            });
        }

        crate::log_info!(
            ctx,
            "legacy_settle",
            "prepare node ready, agent_id={}, trace_id={}, items={}",
            agent_id,
            trace_id,
            ids.len()
        );

        Ok(NodeOutcome::new(NodeSignal::Yield(SIGNAL_READY.into())))
    }
}

// ==================== Settle：跑一轮沉淀思考 ====================

/// 思考节点：消费当前批次，跑一轮沉淀；仅 `Final` 才写回状态
#[derive(Debug)]
pub struct SettleNode {
    deps: Arc<SettleDeps>,
}

impl SettleNode {
    pub fn new(deps: Arc<SettleDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for SettleNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_SETTLE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Deliberate
    }

    fn purpose(&self) -> &str {
        "执行一轮沉淀思考（工具循环 + 退出策略裁决）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![
                (ContextKey::Prompt, LoadDepth::Full),
                (ContextKey::Tools, LoadDepth::Shallow),
            ],
            reads: vec![ContextKey::Prompt, ContextKey::Tools],
            writes: vec![ContextKey::Artifacts],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let pack = read_bus::<PromptPack>(bus, ContextKey::Prompt)?;
        let tools =
            read_bus::<Vec<crate::models::cortex_types::ToolDescriptor>>(bus, ContextKey::Tools)?;

        let batch = {
            let mut slot = match deps.batch.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            slot.take()
        };
        let Some(batch) = batch else {
            // 拓扑保证 settle 只在 prepare 之后出现；走到这里说明边配错了
            return Err(common::error::err!(
                Internal,
                "legacy_settle: settle node has no pending batch"
            ));
        };

        // 已发起模型调用：无论结果如何都算一次 attempt（统计事件 call_count 口径）
        if let Ok(mut agg) = deps.aggregate.lock() {
            agg.attempts += 1;
        }

        crate::log_info!(
            &deps.ctx,
            "legacy_settle",
            "settle node start, agent_id={}, trace_id={}, items={}",
            deps.agent.po.id,
            batch.trace.id,
            batch.ids.len()
        );

        let result = deps
            .backend
            .think(
                deps,
                &batch,
                super::backend::SettleThinkInput {
                    initial_messages: pack.initial_messages,
                    tools,
                    max_rounds: deps.options.effective_max_rounds(),
                },
            )
            .await?;

        let PendingBatch { mut trace, ids, .. } = batch;

        // Trace 元数据：input 是整份 prompt，exit_reason 记录本批如何结束
        trace.input = pack.prompt.clone();
        trace.metadata.insert("scene".into(), "settle".into());
        trace.metadata.insert(
            "depended_trace_ids".into(),
            serde_json::to_string(&deps.trace_ids).unwrap_or_default(),
        );
        if let Some(task_id) = deps.ctx.task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = deps.ctx.project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }

        match result {
            ThinkLoopResult::Final { content, .. } => {
                trace.complete(content);
                trace.metadata.insert("exit_reason".into(), "Final".into());
                // 与老 `sleep_and_settle` 同口径：Trace 落库失败要 `?` 传播
                deps.domain
                    .write_thinking_trace(deps.ctx.clone(), trace)
                    .await?;

                // 终态才标记：无效沉淀不翻状态，记忆留给下一次
                match deps
                    .domain
                    .mark_short_term_settled(deps.ctx.clone(), &deps.agent.po.id, &ids)
                    .await
                {
                    Ok(marked) => crate::log_info!(
                        &deps.ctx,
                        "legacy_settle",
                        "batch settled, agent_id={}, items={}, marked={}",
                        deps.agent.po.id,
                        ids.len(),
                        marked
                    ),
                    Err(e) => crate::log_warn!(
                        &deps.ctx,
                        "legacy_settle",
                        "mark settled failed (non-fatal, will retry next run): {:?}",
                        e
                    ),
                }

                if let Ok(mut agg) = deps.aggregate.lock() {
                    agg.batches += 1;
                    agg.memories += ids.len();
                }

                Ok(NodeOutcome::new(NodeSignal::Yield(SIGNAL_NEXT.into())))
            }
            other => {
                let exit = match &other {
                    ThinkLoopResult::ContextOverflow { .. } => EXIT_OVERFLOW,
                    ThinkLoopResult::MaxRoundsExceeded { .. } => {
                        crate::service::domain::runtime::legacy_awaken::nodes::EXIT_MAX_ROUNDS
                    }
                    ThinkLoopResult::Cancelled { .. } => EXIT_CANCELLED,
                    ThinkLoopResult::Final { .. } => unreachable!("handled above"),
                };
                trace.complete(String::new());
                trace
                    .metadata
                    .insert("exit_reason".into(), exit.to_string());
                deps.domain
                    .write_thinking_trace(deps.ctx.clone(), trace)
                    .await?;

                crate::log_warn!(
                    &deps.ctx,
                    "legacy_settle",
                    "batch blocked, agent_id={}, items={}, exit={}",
                    deps.agent.po.id,
                    ids.len(),
                    exit
                );

                if let Ok(mut agg) = deps.aggregate.lock() {
                    agg.blocked = Some(exit.to_string());
                }

                Ok(NodeOutcome::new(NodeSignal::Yield(SIGNAL_BLOCKED.into())))
            }
        }
    }
}

// ==================== Finalize：整次 run 收尾 ====================

/// 收尾节点：把整次 run 的聚合结果写成一条统计事件 + 一条 AOP 完成事件
///
/// ⚠️ 无待沉淀记忆时**不发任何事件**：与老行为一致（空跑不产生监控噪声），
/// 锚点 trace 为空就是判据。
#[derive(Debug)]
pub struct FinalizeNode {
    deps: Arc<SettleDeps>,
}

impl FinalizeNode {
    pub fn new(deps: Arc<SettleDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for FinalizeNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_FINALIZE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Consolidate
    }

    fn purpose(&self) -> &str {
        "收尾：聚合统计 + 循环完成事件"
    }

    fn contract(&self) -> NodeContract {
        NodeContract::default()
    }

    async fn run(&self, _bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let anchor = {
            let guard = match deps.run_anchor.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.clone()
        };
        let Some(anchor) = anchor else {
            return Ok(NodeOutcome::new(NodeSignal::Done));
        };

        let agg = lock_aggregate(deps);
        let duration_ms = deps.started_at.elapsed().as_millis() as u64;
        let status = match &agg.blocked {
            Some(exit) => format!("settle blocked: {exit}"),
            None => "settle success".to_string(),
        };

        if let Err(stats_err) = crate::record_event!(
            deps.ctx,
            AgentAwakeEvent {
                agent_id: deps.agent.po.id.clone(),
                project_id: None,
                task_id: None,
                organization_id: deps.ctx.organization_id.clone(),
                user_id: Some(deps.ctx.uid()),
                message_id: None,
                call_count: agg.attempts as u64,
                duration_ms: duration_ms,
                status: status.clone(),
                exit_reason: "settle".to_string(),
            }
        ) {
            crate::log_warn!(
                &deps.ctx,
                "legacy_settle",
                "record_event failed, stats may be incomplete: {:?}",
                stats_err
            );
        }

        let _ = crate::pkg::aop::publish(
            &deps.ctx,
            AgentLoopEvent::finished(
                &deps.agent.po.id,
                &anchor,
                "settle",
                &status,
                duration_ms,
                None,
            ),
        )
        .await;

        crate::log_info!(
            &deps.ctx,
            "legacy_settle",
            "finalize done, agent_id={}, batches={}, memories={}, attempts={}, blocked={:?}",
            deps.agent.po.id,
            agg.batches,
            agg.memories,
            agg.attempts,
            agg.blocked
        );

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
