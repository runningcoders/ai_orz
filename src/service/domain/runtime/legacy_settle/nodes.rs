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

use super::context::{PendingBatch, SettleAggregate, SettleDeps};
use super::pending::build_pending_memories_summary;
use crate::models::events::AgentLoopEvent;
use crate::models::memory::MemoryTrace;
use crate::pkg::stats::AgentAwakeEvent;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::awakening::{
    init_think_runtime_and_policy, mount_ontology_lexicon,
};
use crate::service::domain::runtime::legacy_awaken::context::{NodeBase, read_bus};
use crate::service::domain::runtime::legacy_awaken::nodes::{
    EXIT_CANCELLED, EXIT_OVERFLOW, PromptPack, WorkspacePaths,
};
use crate::service::domain::runtime::types::ThinkLoopResult;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeMemory};
use async_trait::async_trait;
use common::enums::{MemoryRole, ThinkingScene};
use common::error::Result;
use std::sync::Arc;
use std::time::Instant;

/// 收尾节点的 id（与 `runner::ensure_finalized` 共用同一真相）
pub(crate) const NODE_FINALIZE: &str = "finalize";
pub(crate) const NODE_PREPARE: &str = "prepare";
pub(crate) const NODE_SETTLE: &str = "settle";

/// 节点出口信号
pub(crate) const SIGNAL_READY: &str = "ready";
pub(crate) const SIGNAL_NEXT: &str = "next";
pub(crate) const SIGNAL_EMPTY: &str = "empty";
pub(crate) const SIGNAL_BLOCKED: &str = "blocked";

/// 标记未生效的阻断原因（与 `EXIT_*` 平级，但来源是存储层而非模型侧退出信号）
///
/// ⚠️ 这个值同时是统计事件 `status` 与 AOP finished 事件的口径，
/// 改动会影响监控看板，不要改写成更好看的形式。
pub(crate) const MARK_SETTLE_FAILED: &str = "MarkSettledFailed";

// ==================== 节点输入契约（节点侧定义）====================
//
// 与 `legacy_awaken` 同一套理念：节点声明自己需要的图能力，图负责把自己的
// 承载结构（这里是 `SettleDeps`）适配成 trait 再交给节点。

/// `PrepareNode`（沉淀）对图输入的能力声明
#[async_trait]
pub(crate) trait SettlePrepareNodeInput: NodeBase {
    fn budget_tokens(&self) -> usize;
    fn item_cap(&self) -> usize;
    fn trace_ids(&self) -> &[String];
    fn domain(&self) -> &RuntimeDomainImpl;
    /// 首个批次登记为本 run 的锚点；返回是否为首批
    fn register_run_anchor(&self, trace_id: &str) -> bool;
    /// 写入当前批次，交给 `SettleNode` 消费
    fn set_batch(&self, batch: PendingBatch);
}

/// `SettleNode` 对图输入的能力声明
#[async_trait]
pub(crate) trait SettleNodeInput: NodeBase {
    fn take_batch(&self) -> Option<PendingBatch>;
    fn note_attempt(&self);
    fn note_memories(&self, marked: usize);
    fn note_batch(&self);
    fn mark_blocked(&self, reason: &str);
    fn trace_ids(&self) -> &[String];
    fn max_rounds(&self) -> usize;
    fn domain(&self) -> &RuntimeDomainImpl;
    /// 执行一轮沉淀思考
    async fn think(
        &self,
        batch: &PendingBatch,
        input: super::backend::SettleThinkInput,
    ) -> Result<ThinkLoopResult>;
}

/// `FinalizeNode` 对图输入的能力声明
pub(crate) trait SettleFinalizeNodeInput: NodeBase {
    fn run_anchor(&self) -> Option<String>;
    fn aggregate(&self) -> SettleAggregate;
    fn started_at(&self) -> Instant;
}

impl NodeBase for SettleDeps {
    fn ctx(&self) -> &crate::pkg::request_context::RequestContext {
        &self.ctx
    }

    fn agent(&self) -> &crate::models::agent::Agent {
        &self.agent
    }
}

#[async_trait]
impl SettlePrepareNodeInput for SettleDeps {
    fn budget_tokens(&self) -> usize {
        self.budget_tokens
    }

    fn item_cap(&self) -> usize {
        SettleDeps::item_cap(self)
    }

    fn trace_ids(&self) -> &[String] {
        &self.trace_ids
    }

    fn domain(&self) -> &RuntimeDomainImpl {
        &self.domain
    }

    fn register_run_anchor(&self, trace_id: &str) -> bool {
        let mut anchor = match self.run_anchor.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if anchor.is_none() {
            *anchor = Some(trace_id.to_string());
            true
        } else {
            false
        }
    }

    fn set_batch(&self, batch: PendingBatch) {
        let mut slot = match self.batch.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = Some(batch);
    }
}

#[async_trait]
impl SettleNodeInput for SettleDeps {
    fn take_batch(&self) -> Option<PendingBatch> {
        let mut slot = match self.batch.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        slot.take()
    }

    fn note_attempt(&self) {
        if let Ok(mut agg) = self.aggregate.lock() {
            agg.attempts += 1;
        }
    }

    fn note_memories(&self, marked: usize) {
        if let Ok(mut agg) = self.aggregate.lock() {
            agg.memories += marked;
        }
    }

    fn note_batch(&self) {
        if let Ok(mut agg) = self.aggregate.lock() {
            agg.batches += 1;
        }
    }

    fn mark_blocked(&self, reason: &str) {
        if let Ok(mut agg) = self.aggregate.lock() {
            agg.blocked = Some(reason.to_string());
        }
    }

    fn trace_ids(&self) -> &[String] {
        &self.trace_ids
    }

    fn max_rounds(&self) -> usize {
        self.options.effective_max_rounds()
    }

    fn domain(&self) -> &RuntimeDomainImpl {
        &self.domain
    }

    async fn think(
        &self,
        batch: &PendingBatch,
        input: super::backend::SettleThinkInput,
    ) -> Result<ThinkLoopResult> {
        self.backend.think(self, batch, input).await
    }
}

impl SettleFinalizeNodeInput for SettleDeps {
    fn run_anchor(&self) -> Option<String> {
        let guard = match self.run_anchor.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    }

    fn aggregate(&self) -> SettleAggregate {
        self.aggregate.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn started_at(&self) -> Instant {
        self.started_at
    }
}

// ==================== Prepare：取一批 + 装配 Prompt ====================

/// 装配节点：查一批未沉淀记忆，产出本批的 Prompt / Trace / 思考运行时
#[derive(Debug)]
pub struct PrepareNode {
    input: Arc<dyn SettlePrepareNodeInput>,
}

impl PrepareNode {
    pub fn new(input: Arc<dyn SettlePrepareNodeInput>) -> Self {
        Self { input }
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
        let input = &self.input;
        let ctx = input.ctx();
        let agent_id = input.agent().po.id.clone();

        crate::log_info!(
            ctx,
            "legacy_settle",
            "prepare node start, agent_id={}, budget_tokens={}, item_cap={}",
            agent_id,
            input.budget_tokens(),
            input.item_cap()
        );

        // 取一批：无候选即本轮沉淀结束
        let Some((summary, ids, _truncated)) = build_pending_memories_summary(
            ctx,
            input.domain().memory(),
            &agent_id,
            input.budget_tokens(),
            input.item_cap(),
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
            init_think_runtime_and_policy(input.agent(), ThinkingScene::Settle, &trace_id);

        // 首个批次成为本次 run 的锚点：AOP started 只发一次
        let is_first = input.register_run_anchor(&trace_id);
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
        let settled_reference = input
            .domain()
            .query_settled_reference(ctx.clone(), &agent_id)
            .await;

        let mut builder = input.domain().prompt_builder(input.agent());
        builder.current_trace_id(&trace_id);
        builder.system_prompt(input.agent());
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
            prompt: builder.build_sleep_prompt(&summary, input.trace_ids()),
            initial_messages: builder.build_sleep_initial_messages(&summary, input.trace_ids()),
        };
        let value = serde_json::to_value(&pack)
            .map_err(|e| common::error::err!(Internal, "encode prompt pack failed: {e}"))?;
        bus.insert(ContextKey::Prompt, LoadDepth::Full, value);

        // 批次写入共享槽位，交给 settle 节点消费
        input.set_batch(PendingBatch {
            trace,
            ids: ids.clone(),
            think_runtime,
            policy,
        });

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
    input: Arc<dyn SettleNodeInput>,
}

impl SettleNode {
    pub fn new(input: Arc<dyn SettleNodeInput>) -> Self {
        Self { input }
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
        let input = &self.input;
        let pack = read_bus::<PromptPack>(bus, ContextKey::Prompt)?;
        let tools =
            read_bus::<Vec<crate::models::cortex_types::ToolDescriptor>>(bus, ContextKey::Tools)?;

        let Some(batch) = input.take_batch() else {
            // 拓扑保证 settle 只在 prepare 之后出现；走到这里说明边配错了
            return Err(common::error::err!(
                Internal,
                "legacy_settle: settle node has no pending batch"
            ));
        };

        // 已发起模型调用：无论结果如何都算一次 attempt（统计事件 call_count 口径）
        input.note_attempt();

        crate::log_info!(
            &input.ctx(),
            "legacy_settle",
            "settle node start, agent_id={}, trace_id={}, items={}",
            input.agent().po.id,
            batch.trace.id,
            batch.ids.len()
        );

        let result = input
            .think(
                &batch,
                super::backend::SettleThinkInput {
                    initial_messages: pack.initial_messages,
                    tools,
                    max_rounds: input.max_rounds(),
                },
            )
            .await?;

        let PendingBatch { mut trace, ids, .. } = batch;

        // Trace 元数据：input 是整份 prompt，exit_reason 记录本批如何结束
        trace.input = pack.prompt.clone();
        trace.metadata.insert("scene".into(), "settle".into());
        trace.metadata.insert(
            "depended_trace_ids".into(),
            serde_json::to_string(input.trace_ids()).unwrap_or_default(),
        );
        if let Some(task_id) = input.ctx().task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = input.ctx().project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }

        match result {
            ThinkLoopResult::Final { content, .. } => {
                trace.complete(content);
                trace.metadata.insert("exit_reason".into(), "Final".into());
                // 与老 `sleep_and_settle` 同口径：Trace 落库失败要 `?` 传播
                input
                    .domain()
                    .write_thinking_trace(input.ctx().clone(), trace)
                    .await?;

                // 终态才标记：无效沉淀不翻状态，记忆留给下一次
                //
                // ⚠️⚠️ 循环排空**只**依赖这里真正把状态翻掉：模型调用成功（`Final`）
                // 不等于本批已沉淀。`mark_short_term_settled` 会把
                // `update_short_term_index` 的失败吞成 `Ok(marked)`（内部仅 log_warn），
                // 因此「标记没生效」绝大多数表现为 `Ok(0)` / `Ok(<ids.len())`，**不是 Err**。
                // 不看 `marked` 就回环，等于把同一批 Active 记忆反复取回重打模型，
                // 直到 MAX_SETTLE_BATCHES 打满 → ensure_finalized 报错 → nack 重投雪崩。
                let marked = match input
                    .domain()
                    .mark_short_term_settled(input.ctx().clone(), &input.agent().po.id, &ids)
                    .await
                {
                    Ok(marked) => marked,
                    Err(e) => {
                        crate::log_warn!(
                            &input.ctx(),
                            "legacy_settle",
                            "mark settled failed (will retry next run): {:?}",
                            e
                        );
                        0
                    }
                };

                // 统计只认**真正翻掉状态**的条数：谎报会让监控把「全批失败」看成成功。
                // 即便本批随后被判为阻断，已标记成功的那些也确实沉淀了，必须如实计入。
                input.note_memories(marked);

                // 标记未生效 ⇒ 本轮已无法再排空：走 blocked 干净收尾，绝不回环。
                // 与 `EXIT_*` 阻断同源：继续跑只会反复选中同一批记忆。
                if marked < ids.len() {
                    crate::log_warn!(
                        &input.ctx(),
                        "legacy_settle",
                        "batch settle incomplete, agent_id={}, items={}, marked={}, stopping run to avoid re-picking the same batch",
                        input.agent().po.id,
                        ids.len(),
                        marked
                    );
                    input.mark_blocked(MARK_SETTLE_FAILED);
                    return Ok(NodeOutcome::new(NodeSignal::Yield(SIGNAL_BLOCKED.into())));
                }

                crate::log_info!(
                    &input.ctx(),
                    "legacy_settle",
                    "batch settled, agent_id={}, items={}, marked={}",
                    input.agent().po.id,
                    ids.len(),
                    marked
                );

                // 注意：`agg.memories` 已在上面的统一块里按 `marked` 累加过，
                // 这里只记批次数，避免重复累加导致监控谎报。
                input.note_batch();

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
                input
                    .domain()
                    .write_thinking_trace(input.ctx().clone(), trace)
                    .await?;

                crate::log_warn!(
                    &input.ctx(),
                    "legacy_settle",
                    "batch blocked, agent_id={}, items={}, exit={}",
                    input.agent().po.id,
                    ids.len(),
                    exit
                );

                input.mark_blocked(exit);

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
    input: Arc<dyn SettleFinalizeNodeInput>,
}

impl FinalizeNode {
    pub fn new(input: Arc<dyn SettleFinalizeNodeInput>) -> Self {
        Self { input }
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
        let input = &self.input;
        let Some(anchor) = input.run_anchor() else {
            return Ok(NodeOutcome::new(NodeSignal::Done));
        };

        let agg = input.aggregate();
        let duration_ms = input.started_at().elapsed().as_millis() as u64;
        let status = match &agg.blocked {
            Some(exit) => format!("settle blocked: {exit}"),
            None => "settle success".to_string(),
        };

        if let Err(stats_err) = crate::record_event!(
            input.ctx(),
            AgentAwakeEvent {
                agent_id: input.agent().po.id.clone(),
                project_id: None,
                task_id: None,
                organization_id: input.ctx().organization_id.clone(),
                user_id: Some(input.ctx().uid()),
                message_id: None,
                call_count: agg.attempts as u64,
                duration_ms: duration_ms,
                status: status.clone(),
                exit_reason: "settle".to_string(),
            }
        ) {
            crate::log_warn!(
                &input.ctx(),
                "legacy_settle",
                "record_event failed, stats may be incomplete: {:?}",
                stats_err
            );
        }

        let _ = crate::pkg::aop::publish(
            input.ctx(),
            AgentLoopEvent::finished(
                &input.agent().po.id,
                &anchor,
                "settle",
                &status,
                duration_ms,
                None,
            ),
        )
        .await;

        crate::log_info!(
            &input.ctx(),
            "legacy_settle",
            "finalize done, agent_id={}, batches={}, memories={}, attempts={}, blocked={:?}",
            input.agent().po.id,
            agg.batches,
            agg.memories,
            agg.attempts,
            agg.blocked
        );

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
