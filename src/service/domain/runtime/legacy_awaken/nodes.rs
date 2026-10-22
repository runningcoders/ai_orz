//! `legacy_awaken` 的四个节点
//!
//! 这是把 `awakening.rs` 的 awaitaken 主循环（含 93 行装配）**按职责切碎**的结果。
//! 第一版刻意保持**行为等价**：每个节点内部仍调用老链路的能力，只是重新划分边界。
//! 逻辑本身的下沉（把 480 行 `run_think_loop` 拆成工具节点/守护节点）是后续的事。
//!
//! 节点间只通过总线通信，唯一的例外是 `AwakenDeps`（不可序列化的强类型依赖）。

use super::context::{AwakenDeps, read_bus};
use crate::pkg::stats::AgentAwakeEvent;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::RuntimeMemory;
use crate::service::domain::runtime::types::ThinkLoopResult;
use async_trait::async_trait;
use common::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// 历史短期记忆条数（与老链路一致）
const RECENT_MEMORIES_LIMIT: usize = 20;

/// 收尾节点的 id
///
/// ⚠️ 必须是单一真相：`ConsolidateNode::id()` 产出它、蓝图把它声明为终点、
/// `runner` 的终止护栏凭它判断「这次跑到底了没有」。三处各写一份字面量时，
/// 改名漏一处就会让护栏把正常结束误判成失败。
pub(crate) const NODE_CONSOLIDATE: &str = "consolidate";

// ==================== 出口信号与退出语义 ====================
//
// ⚠️⚠️ 这些常量不是为了「换个写法」，是为了消灭**四份互不相干的手写真相**：
// `map_think_result` 产出的 Tag、蓝图 `EdgeSpec.on` 匹配的 Tag、
// `Artifacts.exit` 的取值、`Consolidate` 的分支判断。
//
// 原先这个 Match 内联在 `async fn run` 里，四处的 `"overflow"` / `"max_rounds"` /
// `"cancelled"` 各写各的。改一处漏一处时**测试仍然全绿**，但运行时边匹配不上，
// run 会因无候选而静默终止 —— 不写 Trace、不报统计、不发事件，日志里也看不出来。
// 这正是设计文档里批评老链路「Match 分支漏一个就是静默丢路径」的原样复发。

/// `Deliberate` 产出信号的 Tag（蓝图边匹配的就是它）
pub(crate) const SIGNAL_OVERFLOW: &str = "overflow";
pub(crate) const SIGNAL_MAX_ROUNDS: &str = "max_rounds";
pub(crate) const SIGNAL_CANCELLED: &str = "cancelled";

/// `Artifacts.exit` 的取值：`Consolidate` 凭它决定做什么收尾
///
/// ⚠️ 这组取值同时是 **Trace 元数据 `exit_reason`** 和 **`AgentAwakeEvent.exit_reason`**
/// 的口径，必须与老 `awaken` 逐字节一致。换成「更好看」的写法会让同一份行为在
/// 监控里分裂成两个标签（如 `max_rounds` vs `maxroundsexceeded`）—— 既不报错
/// 也不会让测试失败，聚合却在静默失真。
pub(crate) const EXIT_FINAL: &str = "Final";
pub(crate) const EXIT_MAX_ROUNDS: &str = "MaxRoundsExceeded";
pub(crate) const EXIT_CANCELLED: &str = "cancelled";

/// 只有压缩回环内部使用（`overflow` → compact → prepare）：下一次思考的结果会
/// 覆盖它，**永远不会被送到收尾节点**，因此不参与上面的监控口径。
pub(crate) const EXIT_OVERFLOW: &str = "overflow";

// ==================== 总线上的载荷结构 ====================

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WorkspacePaths {
    pub default: String,
    pub user_home: String,
    pub user_shared: String,
    pub user_agent: Option<String>,
    pub agent: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BusinessContext {
    pub message_thread: Vec<String>,
    pub resource_lines: Vec<String>,
    pub vision_parts: Vec<crate::models::cortex_types::ImagePart>,
    pub past_memories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PromptPack {
    pub prompt: String,
    pub initial_messages: Vec<crate::models::cortex_types::ChatMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BudgetState {
    pub total_rounds: usize,
}

/// 退出来由与本轮的产出：Consolidate 全靠它决定做什么收尾
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Artifacts {
    /// `final` / `max_rounds` / `cancelled`
    pub exit: String,
    pub content: String,
    pub messages: Vec<crate::models::cortex_types::ChatMessage>,
}

fn read_budget(bus: &ThinkBus) -> BudgetState {
    read_bus::<BudgetState>(bus, ContextKey::Budget).unwrap_or_default()
}

fn read_artifacts(bus: &ThinkBus) -> Result<Artifacts> {
    read_bus::<Artifacts>(bus, ContextKey::Artifacts)
}

fn write_artifacts(bus: &mut ThinkBus, artifacts: &Artifacts) -> Result<()> {
    let value = serde_json::to_value(artifacts)
        .map_err(|e| common::error::err!(Internal, "encode artifacts failed: {e}"))?;
    bus.insert(ContextKey::Artifacts, LoadDepth::Full, value);
    Ok(())
}

// ==================== Prepare：统一装配 ====================

/// 装配节点：把散落在老 awaitaken 里的十余次查询收拢到一个地方
///
/// 之所以**不把 awaitaken 的 prompt 装配整个做成 ContextProvider**：
/// 装配要跨分区合成（`history()` 吃 `&[Memory]`，且 `Memory` 不可序列化），
/// 而 Provider 的接口只负责「单一分区的取数」。跨分区合成是节点的活。
#[derive(Debug)]
pub struct PrepareNode {
    deps: Arc<AwakenDeps>,
    id: NodeId,
}

impl PrepareNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self {
            deps,
            id: NodeId::new("prepare"),
        }
    }
}

#[async_trait]
impl ThinkNode for PrepareNode {
    fn id(&self) -> NodeId {
        self.id.clone()
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Prepare
    }

    fn purpose(&self) -> &str {
        "装配本轮 Prompt：记忆 / 工具 / 技能 / 工作空间 / 业务上下文"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![
                (ContextKey::Runtime, LoadDepth::Shallow),
                (ContextKey::Skills, LoadDepth::Shallow),
                (ContextKey::Business, LoadDepth::Full),
            ],
            reads: vec![
                ContextKey::Runtime,
                ContextKey::Tools,
                ContextKey::Skills,
                ContextKey::Business,
            ],
            writes: vec![ContextKey::Prompt, ContextKey::Budget],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let ctx = &deps.ctx;

        crate::log_info!(
            ctx,
            "legacy_awaken",
            "prepare node start, agent_id={}",
            deps.agent.po.id
        );

        let slot = deps
            .compaction
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        // 唯二之一不能进总线的取数：短期记忆实体（`builder.history` 要 `&[Memory]`）
        let recent: Vec<crate::models::memory::Memory> = if slot.summary.is_some() {
            Vec::new()
        } else {
            deps.domain
                .get_recent_context(ctx.clone(), &deps.agent.po.id, RECENT_MEMORIES_LIMIT)
                .await?
        };

        crate::log_info!(
            ctx,
            "legacy_awaken",
            "recent context loaded, agent_id={}, n={}",
            deps.agent.po.id,
            recent.len()
        );

        let ws = read_bus::<WorkspacePaths>(bus, ContextKey::Runtime)?;
        let skills = read_bus::<Vec<crate::models::skill::SkillPo>>(bus, ContextKey::Skills)?;
        let biz = read_bus::<BusinessContext>(bus, ContextKey::Business)?;

        let mut builder = deps.domain.prompt_builder(&deps.agent);
        builder.current_trace_id(&deps.trace_id);
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
        if let Some(project) = &deps.options.project {
            builder.project_context(project);
        }
        if let Some(task) = &deps.options.task {
            builder.task_context(task);
        }
        if let Some(user) = &deps.options.user_profile {
            builder.user_profile(user);
        }
        super::super::awakening::mount_ontology_lexicon(ctx, builder.as_mut()).await;
        builder.history(&recent);
        if let Some(summary) = &slot.summary {
            builder.compacted_context(summary);
        }
        if !biz.past_memories.is_empty() {
            builder.past_memories_reference(&biz.past_memories);
        }
        if !biz.resource_lines.is_empty() {
            builder.set_resource_context_lines(biz.resource_lines);
        }
        if !biz.vision_parts.is_empty() {
            builder.set_current_message_vision(biz.vision_parts);
        }
        builder.current_message(&deps.message);
        builder.message_thread(&biz.message_thread);

        let pack = PromptPack {
            prompt: builder.build(),
            initial_messages: builder.build_initial_messages(),
        };
        let value = serde_json::to_value(&pack)
            .map_err(|e| common::error::err!(Internal, "encode prompt pack failed: {e}"))?;
        bus.insert(ContextKey::Prompt, LoadDepth::Full, value);

        let budget = BudgetState {
            total_rounds: read_budget(bus).total_rounds,
        };
        bus.insert(
            ContextKey::Budget,
            LoadDepth::Shallow,
            serde_json::to_value(budget)
                .map_err(|e| common::error::err!(Internal, "encode budget failed: {e}"))?,
        );

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}

// ==================== Deliberate：跑一轮思考 ====================

/// 一次翻译的产物：`Deliberate` 把 `ThinkLoopResult` 翻译成节点信号 + 语义标签
#[derive(Debug, Clone)]
pub(crate) struct ExitMapping {
    pub signal: NodeSignal,
    /// 语义标签，写进 `Artifacts.exit`，`Consolidate` 凭它决定做什么收尾
    pub exit: &'static str,
    pub content: String,
    pub messages: Vec<crate::models::cortex_types::ChatMessage>,
    /// 回写 `ContextKey::Budget` 的累计轮次
    pub rounds: usize,
}

/// `ThinkLoopResult` → `ExitMapping`
///
/// ⚠️ **节点信号 Tag 的唯一真相来源**。抽成纯函数不是洁癖：
/// 原先内联在 `async fn run` 里，`tests.rs` 只能靠一个手写字面量数组去断言
/// 「这些 Tag 有对应的边」—— 那本身就是第三份真相，改了映射它还绿。
/// 现在测试可以从四个变体直接推导出 Tag 再去蓝图里找边（见 `tests.rs`）。
pub(crate) fn map_think_result(result: ThinkLoopResult, total_rounds: usize) -> ExitMapping {
    match result {
        // ⚠️ `rounds` 必须与老 `awaken` 同口径：Final 分支在老代码里**不更新**
        // `total_rounds`（只有 overflow / max_rounds 才累加）。写回 `total_rounds`
        // 才能保住跨压缩累计的值；写成 `max_rounds` 会让 trace 的 `rounds_used`
        // 凭空变成上限值。
        ThinkLoopResult::Final { content, messages } => ExitMapping {
            signal: NodeSignal::Done,
            exit: EXIT_FINAL,
            content,
            messages,
            rounds: total_rounds,
        },
        ThinkLoopResult::ContextOverflow {
            messages,
            rounds_used,
            ..
        } => ExitMapping {
            signal: NodeSignal::Yield(SIGNAL_OVERFLOW.into()),
            exit: EXIT_OVERFLOW,
            content: String::new(),
            messages,
            rounds: total_rounds.saturating_add(rounds_used),
        },
        ThinkLoopResult::MaxRoundsExceeded {
            messages,
            total_rounds: rounds,
        } => ExitMapping {
            signal: NodeSignal::Yield(SIGNAL_MAX_ROUNDS.into()),
            exit: EXIT_MAX_ROUNDS,
            content: String::new(),
            messages,
            rounds,
        },
        // ⚠️ `rounds` 要与老 `awaken` 同口径：取消分支在那里是
        // `total_rounds.saturating_add(rounds)` —— 被打断那一段也算做过。
        // 只写回 `total_rounds` 会让存档少算一轮。
        ThinkLoopResult::Cancelled {
            messages,
            total_rounds: rounds,
        } => ExitMapping {
            signal: NodeSignal::Yield(SIGNAL_CANCELLED.into()),
            exit: EXIT_CANCELLED,
            content: String::new(),
            messages,
            rounds: total_rounds.saturating_add(rounds),
        },
    }
}

/// 每个 `ThinkLoopResult` 变体一个样例：测试用它穷举，而不是手写字面量数组
#[cfg(test)]
pub(crate) fn sample_results(total_rounds: usize) -> Vec<ThinkLoopResult> {
    vec![
        ThinkLoopResult::Final {
            content: "ok".to_string(),
            messages: Vec::new(),
        },
        ThinkLoopResult::ContextOverflow {
            messages: Vec::new(),
            input_tokens: 0,
            rounds_used: 1,
        },
        ThinkLoopResult::MaxRoundsExceeded {
            messages: Vec::new(),
            total_rounds,
        },
        ThinkLoopResult::Cancelled {
            messages: Vec::new(),
            total_rounds,
        },
    ]
}

/// 思考节点：老链路里「一次 awaitaken 循环」的位置
///
/// 第一版内部仍是 `run_think_loop`（480 行，含小脑路由、工具二分、8 个退出策略），
/// 此处只负责把它出现的结果翻译成**节点信号** —— 跳转让边决定，而不是栗子 Match 外传。
#[derive(Debug)]
pub struct DeliberateNode {
    deps: Arc<AwakenDeps>,
}

impl DeliberateNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for DeliberateNode {
    fn id(&self) -> NodeId {
        NodeId::new("deliberate")
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Deliberate
    }

    fn purpose(&self) -> &str {
        "执行一轮完整思考（工具循环 + 退出策略裁决）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![
                (ContextKey::Prompt, LoadDepth::Full),
                (ContextKey::Tools, LoadDepth::Shallow),
            ],
            reads: vec![ContextKey::Prompt, ContextKey::Tools, ContextKey::Budget],
            writes: vec![ContextKey::Artifacts, ContextKey::Budget],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let pack = read_bus::<PromptPack>(bus, ContextKey::Prompt)?;
        let tools =
            read_bus::<Vec<crate::models::cortex_types::ToolDescriptor>>(bus, ContextKey::Tools)?;
        let total_rounds = read_budget(bus).total_rounds;
        let max_rounds = deps.options.effective_max_rounds();

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "deliberate node start, agent_id={}, max_rounds={}, total_rounds={}",
            deps.agent.po.id,
            max_rounds,
            total_rounds
        );

        let result = deps
            .backend
            .think(
                deps,
                super::backend::ThinkInput {
                    initial_messages: pack.initial_messages,
                    tools,
                    max_rounds,
                    total_rounds,
                },
            )
            .await;

        let ExitMapping {
            signal,
            exit,
            content,
            messages,
            rounds,
        } = map_think_result(result?, total_rounds);

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "deliberate node done, agent_id={}, exit={}, rounds={}",
            deps.agent.po.id,
            exit,
            rounds
        );

        bus.insert(
            ContextKey::Budget,
            LoadDepth::Shallow,
            json!({ "total_rounds": rounds }),
        );
        write_artifacts(
            bus,
            &Artifacts {
                exit: exit.to_string(),
                content,
                messages,
            },
        )?;
        Ok(NodeOutcome::new(signal))
    }
}

// ==================== Compact：上下文压缩 ====================

/// 压缩节点：把当前对话压成一条短期记忆
///
/// `with_user_reply = true` 时压缩产物剥掉摘要标记后**作为回复发给用户**
/// （轮次耗尽退出流程），否则只是内部产物，压完回到 `Prepare` 继续下一轮。
#[derive(Debug)]
pub struct CompactNode {
    deps: Arc<AwakenDeps>,
    id: NodeId,
    with_user_reply: bool,
}

impl CompactNode {
    /// 溢出压缩：压完回 `Prepare` 重开一轮
    pub fn overflow(deps: Arc<AwakenDeps>) -> Self {
        Self {
            deps,
            id: NodeId::new("compact"),
            with_user_reply: false,
        }
    }

    /// 轮次耗尽退出：压缩的 Final 文本就是发给用户的内容
    pub fn forced(deps: Arc<AwakenDeps>) -> Self {
        Self {
            deps,
            id: NodeId::new("force_compact"),
            with_user_reply: true,
        }
    }
}

#[async_trait]
impl ThinkNode for CompactNode {
    fn id(&self) -> NodeId {
        self.id.clone()
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Prepare
    }

    fn purpose(&self) -> &str {
        "上下文压缩：把本轮对话沉淀为短期记忆"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: Vec::new(),
            reads: vec![ContextKey::Artifacts],
            writes: vec![ContextKey::Artifacts, ContextKey::Business],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "compact node start, agent_id={}, with_user_reply={}",
            deps.agent.po.id,
            self.with_user_reply
        );
        let mut artifacts = read_artifacts(bus)?;
        let trace_ids = vec![deps.trace_id.clone()];

        let mut pending_slot = None;
        match deps
            .backend
            .compact(
                deps,
                super::backend::CompactInput {
                    messages: artifacts.messages.clone(),
                    trace_ids: trace_ids.clone(),
                    with_user_reply: self.with_user_reply,
                },
            )
            .await
        {
            Ok(outcome) => {
                if self.with_user_reply {
                    let text = super::super::compaction::strip_summary_block(&outcome.final_text);
                    artifacts.content = if text.is_empty() {
                        "任务因思考轮次耗尽而终止，已执行上下文压缩退出流程。".to_string()
                    } else {
                        text
                    };
                    artifacts.exit = EXIT_MAX_ROUNDS.to_string();
                } else {
                    deps.progress.reset();
                    pending_slot = Some(super::context::CompactionSlot {
                        summary: outcome.compacted_summary,
                        memory_id: outcome.compacted_memory_id,
                    });
                }
            }
            Err(e) => {
                crate::log_warn!(
                    &deps.ctx,
                    "legacy_awaken",
                    "compaction failed: {:?}, continuing",
                    e
                );
            }
        }

        if let Some(slot) = pending_slot
            && let Ok(mut guard) = deps.compaction.lock()
        {
            *guard = slot;
            // 压缩产物会改变下一轮的取数结果：让 Business / Prompt 失效重算
            bus.invalidate(ContextKey::Business);
            bus.invalidate(ContextKey::Prompt);
        }
        write_artifacts(bus, &artifacts)?;
        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "compact node done, agent_id={}, with_user_reply={}",
            deps.agent.po.id,
            self.with_user_reply
        );
        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}

// ==================== Consolidate：收尾 ====================

/// 收尾节点：写 Trace / 完成后压缩 / 统计 / 循环完成事件
///
/// 老 `awaken` 的 Step 5~8 全在这里。它是**唯一**知道「这次是怎么结束的」的节点，
/// 凭的是总线上的 `Artifacts.exit` —— 不再靠调用方的 Match 分支区分语义。
#[derive(Debug)]
pub struct ConsolidateNode {
    deps: Arc<AwakenDeps>,
}

impl ConsolidateNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for ConsolidateNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_CONSOLIDATE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Consolidate
    }

    fn purpose(&self) -> &str {
        "收尾：写 Trace、产出已完成后压缩、上报统计"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: Vec::new(),
            reads: vec![
                ContextKey::Prompt,
                ContextKey::Artifacts,
                ContextKey::Budget,
            ],
            writes: vec![ContextKey::Artifacts],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let artifacts = read_artifacts(bus)?;
        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "consolidate node start, agent_id={}, exit={}",
            deps.agent.po.id,
            artifacts.exit
        );
        let pack = read_bus::<PromptPack>(bus, ContextKey::Prompt)?;
        let budget = read_budget(bus);
        let duration_ms = deps.started_at.elapsed().as_millis() as u64;

        let mut artifacts = artifacts;
        if artifacts.exit == EXIT_CANCELLED {
            if !deps.shadow {
                let outcome = deps
                    .domain
                    .finalize_abort(super::super::abort_summary::AbortFinalizeParams {
                        ctx: deps.ctx.clone(),
                        agent: &deps.agent,
                        trace: deps
                            .trace
                            .lock()
                            .map(|t| t.clone())
                            // 锁中毒自愈：Mutex 里没有任何可 panic 代码，
                            // 中毒只说明上一任持锁者 panic 了，数据本身仍然可信
                            .unwrap_or_else(|poisoned| poisoned.into_inner().clone()),
                        prompt: &pack.prompt,
                        user_message: deps.message.content(),
                        message_id: &deps.message.po.id,
                        progress: &deps.progress,
                        total_rounds: budget.total_rounds,
                        trace_ids: std::slice::from_ref(&deps.trace_id),
                        kind: super::super::abort_summary::AbortKind::Cancelled,
                        error: None,
                    })
                    .await;
                crate::log_info!(
                    &deps.ctx,
                    "legacy_awaken",
                    "cancelled abort-summary persisted: memory_id={:?}",
                    outcome.memory_id
                );
                let _ = crate::record_event!(
                    deps.ctx,
                    AgentAwakeEvent {
                        agent_id: deps.agent.po.id.clone(),
                        project_id: deps.ctx.project_id().cloned(),
                        task_id: deps.ctx.task_id().cloned(),
                        organization_id: deps.ctx.organization_id.clone(),
                        user_id: Some(deps.ctx.uid()),
                        message_id: Some(deps.message.po.id.clone()),
                        call_count: 1,
                        duration_ms: duration_ms,
                        status: "cancelled".to_string(),
                        exit_reason: EXIT_CANCELLED.to_string(),
                    }
                );
                let _ = crate::pkg::aop::publish(
                    &deps.ctx,
                    crate::models::events::AgentLoopEvent::finished(
                        &deps.agent.po.id,
                        &deps.trace_id,
                        "awaken",
                        "cancelled",
                        duration_ms,
                        Some(&deps.message.po.id),
                    ),
                )
                .await;
            }
            write_artifacts(bus, &artifacts)?;
            return Ok(NodeOutcome::new(NodeSignal::Done));
        }

        if artifacts.content.is_empty() {
            artifacts.content = "任务因思考轮次耗尽而终止，已执行上下文压缩退出流程。".to_string();
        }

        if !deps.shadow {
            // ⚠️ 用 `match` 而不是 `if let`：if-let 的 scrutinee 临时值会活到块结束，
            // 把 `MutexGuard` 拖到 await 上（future 丢 Send）。这里显式拿到 guard、
            // 改完立刻 drop，后面的 await 与锁无关。
            let snapshot = {
                let mut trace = match deps.trace.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                trace.input = pack.prompt.clone();
                trace.complete(artifacts.content.clone());
                trace.metadata.insert("scene".into(), "awaken".into());
                trace
                    .metadata
                    .insert("message_id".into(), deps.message.po.id.clone());
                trace
                    .metadata
                    .insert("exit_reason".into(), artifacts.exit.clone());
                trace
                    .metadata
                    .insert("rounds_used".into(), budget.total_rounds.to_string());
                if let Some(task_id) = deps.ctx.task_id() {
                    trace.metadata.insert("task_id".into(), task_id.clone());
                }
                if let Some(project_id) = deps.ctx.project_id() {
                    trace
                        .metadata
                        .insert("project_id".into(), project_id.clone());
                }
                trace.clone()
            };
            // ⚠️ 与老 `awaken` 同口径：Trace 落库失败要 `?` 传播。
            // 用 `let _ =` 吞掉会把「工作做完了但没留痕」变成一次**假装成功**的
            // 正常返回 —— 用户拿到了回复，运维却查不到任何 trace。
            deps.domain
                .write_thinking_trace(deps.ctx.clone(), snapshot)
                .await?;

            // 正常完成后压缩本次对话为短期记忆；max_rounds 路径已在 CompactNode 压过
            if artifacts.exit == EXIT_FINAL {
                let trace_ids = vec![deps.trace_id.clone()];
                let _ = deps
                    .backend
                    .compact(
                        deps,
                        super::backend::CompactInput {
                            messages: artifacts.messages.clone(),
                            trace_ids: trace_ids.clone(),
                            with_user_reply: false,
                        },
                    )
                    .await
                    .map_err(|e| {
                        crate::log_warn!(
                            &deps.ctx,
                            "legacy_awaken",
                            "post-completion compaction failed: {:?}, continuing (non-fatal)",
                            e
                        );
                    });
            }

            let _ = crate::record_event!(
                deps.ctx,
                AgentAwakeEvent {
                    agent_id: deps.agent.po.id.clone(),
                    project_id: deps.ctx.project_id().cloned(),
                    task_id: deps.ctx.task_id().cloned(),
                    organization_id: deps.ctx.organization_id.clone(),
                    user_id: Some(deps.ctx.uid()),
                    message_id: Some(deps.message.po.id.clone()),
                    call_count: 1,
                    duration_ms: duration_ms,
                    status: "success".to_string(),
                    // 与老 `awaken` 同口径：统计标签是小写的 exit reason
                    exit_reason: artifacts.exit.to_lowercase(),
                }
            );
            let _ = crate::pkg::aop::publish(
                &deps.ctx,
                crate::models::events::AgentLoopEvent::finished(
                    &deps.agent.po.id,
                    &deps.trace_id,
                    "awaken",
                    "success",
                    duration_ms,
                    Some(&deps.message.po.id),
                ),
            )
            .await;
        }

        write_artifacts(bus, &artifacts)?;
        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
