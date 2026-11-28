//! 唤醒图的三处新输入挂点
//!
//! ```text
//! intent ──done──▶ poll_front ──new_input──▶ intent
//!                       └──done──▶ retrieve
//!
//! deliberate ──done──▶ poll_exit ──new_input──▶ intent
//! intent ──direct/clarify──▶ poll_exit ──rethink──▶ prepare
//! force_compact ──done──▶ poll_exit ──exit_done──▶ consolidate
//! ```
//!
//! 三处共用 [`collect`] 收集器收集相关新消息，再由
//! [`inbox_intent_shift`](crate::service::domain::runtime::inbox_intent_shift)
//! 做一次二选一判断决定去向：
//! - `reinterpret`（用户否定了当前意图）→ `set_revision` + `new_input` 回 intent，
//!   带上旧结论 + 新输入做增量修订（与在图中哪个挂点无关）；
//! - `supplement`（仍在补充当前意图）→ `set_pending_turn` 尾插追加；
//!   `poll_exit` 因已过装配阶段，额外走 `rethink` 回 prepare 重装配。
//!
//! 图本身具备回到任意节点的能力，所以不再用「已检索就不能回意图」这类
//! 位置硬约束；判断拿不到结果（无小脑 / 降级）时一律 `supplement` —— 追加
//! 成本最低，不重跑意图、不作废已完成的检索与装配。
//!
//! ## 输入边界（节点侧定义）
//!
//! 节点知道自己要什么、要输出什么；图知道自己有什么。图调度节点时，把自己的
//! 东西适配成 [`PollNodeInput`] 再交给节点。`PollNode` 因此只依赖这条语义契约
//! （不是 `AwakenDeps` 这个具体承载结构），换一张图只要实现本 trait 就能复用。

use super::context::{AwakenDeps, NodeBase, read_bus};
use super::nodes::BudgetState;
use crate::models::cortex_types::ChatMessage;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::inbox::{
    CollectOutcome, InjectedMessage, MAX_ABSORPTIONS_PER_WAKE,
};
use crate::service::domain::runtime::inbox_intent_shift::{
    IntentShift, decide, decide_with_default_dao,
};
use crate::service::domain::runtime::inbox_relevance::InboxJudgeContext;
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 意图后、检索前的挂点 id
pub(crate) const NODE_POLL_FRONT: &str = "poll_front";
/// 落地前的挂点 id
pub(crate) const NODE_POLL_EXIT: &str = "poll_exit";

/// 有新输入且需要回意图修订
pub(crate) const SIGNAL_NEW_INPUT: &str = "new_input";
/// 已执行过一轮，带新输入继续思考（不回退意图）
pub(crate) const SIGNAL_RETHINK: &str = "rethink";

/// 出口检查通过、可以正常收尾
///
/// ⚠️ 不能复用 `Done`：`poll_exit` 的三条出边共享同一个前驱，而引擎的准入
/// 规则按「目标节点」聚合信号集合（前驱 ∪ 信号标签），不按单条边配对。
/// 若收尾也用 `done`，`prepare`（也有 `done` 入边）会同时成为合法候选，
/// 仲裁器就会在「收尾」和「再装配」之间随意选择。唯一信号才能消除歧义。
pub(crate) const SIGNAL_EXIT_DONE: &str = "exit_done";

/// 挂点位置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollKind {
    /// 意图识别后、检索前
    Front,
    /// `final` / `direct` / `clarify` 落地前
    Exit,
}

/// `PollNode` 对「图提供的输入」的声明
///
/// 这是节点侧定义的输入契约：节点只说自己需要的语义能力（身份、预算总账、
/// 收件箱收集与去向判定），不关心这些数据在具体图里由哪个结构承载。图负责
/// 把自己的东西适配成本 trait（参考实现见下方 `impl PollNodeInput for
/// AwakenDeps`），从而让节点可以跨图复用。
#[async_trait]
pub(crate) trait PollNodeInput: NodeBase {
    /// 本场景允许的最大思考轮次
    fn effective_max_rounds(&self) -> usize;

    /// 本唤醒已吸收的新输入批次数
    fn absorptions(&self) -> Result<usize>;

    /// 本唤醒剩余的注入 token 预算（`None` = 无上限）
    fn remaining_inbox_tokens(&self) -> Result<Option<usize>>;

    /// Phase 1 初始消息快照（用于抽取小脑判定现场）
    fn intent_seed_messages(&self) -> Option<Vec<ChatMessage>>;

    /// 收集本挂点可注入的新输入
    async fn collect(&self, judge_ctx: &InboxJudgeContext) -> Result<CollectOutcome>;

    /// 回写本批判定结果与实际用量
    fn record_judged(&self, judged: &[String], used_tokens: usize) -> Result<()>;

    /// 回 intent：推进游标 + 记一次吸收 + 暂存增量修订输入
    fn advance_revision(&self, batch: Vec<InjectedMessage>) -> Result<()>;

    /// 尾插补充：推进游标 + 记一次吸收（`poll_exit` 额外标记续跑追加）
    fn advance_supplement(&self, batch: Vec<InjectedMessage>, resume_append: bool) -> Result<()>;

    /// 二选一判据：本批新输入是「否定意图」还是「补充当前意图」
    async fn judge_shift(
        &self,
        judge_ctx: &InboxJudgeContext,
        batch: &[InjectedMessage],
    ) -> IntentShift;
}

/// 唤醒图把 `AwakenDeps` 适配成 `PollNode` 的输入
///
/// 图知道自己有什么（收件箱运行时、Agent、选项），节点知道自己要什么；
/// 这里就是两者之间的转换层。
#[async_trait]
impl PollNodeInput for AwakenDeps {
    fn effective_max_rounds(&self) -> usize {
        self.options.effective_max_rounds()
    }

    fn absorptions(&self) -> Result<usize> {
        self.runtime.inbox_absorptions()
    }

    fn remaining_inbox_tokens(&self) -> Result<Option<usize>> {
        self.runtime.inbox_remaining_tokens()
    }

    fn intent_seed_messages(&self) -> Option<Vec<ChatMessage>> {
        self.runtime.intent_seed_messages()
    }

    async fn collect(&self, judge_ctx: &InboxJudgeContext) -> Result<CollectOutcome> {
        self.runtime
            .inbox_collect(&self.ctx, judge_ctx, self.agent.brain.as_ref())
            .await
    }

    fn record_judged(&self, judged: &[String], used_tokens: usize) -> Result<()> {
        self.runtime.inbox_record_judged(judged, used_tokens)
    }

    fn advance_revision(&self, batch: Vec<InjectedMessage>) -> Result<()> {
        self.runtime.inbox_advance_revision(batch)
    }

    fn advance_supplement(&self, batch: Vec<InjectedMessage>, resume_append: bool) -> Result<()> {
        self.runtime.inbox_advance_supplement(batch, resume_append)
    }

    async fn judge_shift(
        &self,
        judge_ctx: &InboxJudgeContext,
        batch: &[InjectedMessage],
    ) -> IntentShift {
        match self.intent_shift_dal.as_deref() {
            Some(dal) => decide(&self.ctx, judge_ctx, batch, self.agent.brain.as_ref(), dal).await,
            None => {
                decide_with_default_dao(&self.ctx, judge_ctx, batch, self.agent.brain.as_ref())
                    .await
            }
        }
    }
}

/// 新输入收集节点
#[derive(Debug)]
pub struct PollNode {
    input: Arc<dyn PollNodeInput>,
    kind: PollKind,
}

impl PollNode {
    pub fn new(input: Arc<dyn PollNodeInput>, kind: PollKind) -> Self {
        Self { input, kind }
    }
}

#[async_trait]
impl ThinkNode for PollNode {
    fn id(&self) -> NodeId {
        NodeId::new(match self.kind {
            PollKind::Front => NODE_POLL_FRONT,
            PollKind::Exit => NODE_POLL_EXIT,
        })
    }

    fn phase(&self) -> PhaseId {
        match self.kind {
            PollKind::Front => PhaseId::Triage,
            PollKind::Exit => PhaseId::Review,
        }
    }

    fn purpose(&self) -> &str {
        match self.kind {
            PollKind::Front => "意图后收集新输入：可回退做增量修订",
            PollKind::Exit => "落地前收集新输入：防止发出已作废的直答/澄清",
        }
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![(ContextKey::Budget, LoadDepth::Shallow)],
            reads: vec![
                ContextKey::Budget,
                ContextKey::Intent,
                ContextKey::Knowledge,
            ],
            // 回退 / 转向都会让检索与装配结果失效
            writes: vec![ContextKey::Knowledge, ContextKey::Prompt],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        run_poll(self.input.as_ref(), self.kind, bus).await
    }
}

async fn run_poll(
    input: &dyn PollNodeInput,
    kind: PollKind,
    bus: &mut ThinkBus,
) -> Result<NodeOutcome> {
    // 剩余轮次：只剩最后一轮时不再注入，避免「听完就没轮次回答」
    let available = input.effective_max_rounds().saturating_sub(
        read_bus::<BudgetState>(bus, ContextKey::Budget).map_or(0, |b| b.total_rounds),
    );
    let absorptions = input.absorptions()?;

    // 额度检查上移：吸收次数已满、轮次用尽或预算归零时不再 collect，避免白白
    // DB 查询 + 小脑判定并把候选标成 judged（与 think_round 的 poll 挂点口径一致）。
    if absorptions >= MAX_ABSORPTIONS_PER_WAKE
        || available < 1
        || input.remaining_inbox_tokens()? == Some(0)
    {
        return Ok(match kind {
            PollKind::Front => NodeOutcome::new(NodeSignal::Done),
            PollKind::Exit => finish(),
        });
    }

    // 判定上下文：身份 + 场景 + 任务/项目摘要 + 最近动作，从 Phase 1 消息快照抽取。
    // brain 可能尚未装配（Agent.brain=None）⇒ 片段不截断，行为与容量未配一致。
    let judge_ctx = {
        let seed = input.intent_seed_messages();
        let messages: &[ChatMessage] = seed.as_deref().unwrap_or(&[]);
        crate::service::domain::runtime::inbox_relevance::build_judge_context(
            input.agent(),
            common::enums::ThinkingScene::Awaken,
            messages,
        )
    };

    // 共享参数（锚点 / 游标 / 已判定 / 剩余预算）由运行总线自己取出；
    // 这里只传本次判定的现场。实际用量由总线按 `剩余 -= 用量` 扣回总账。
    let outcome = input.collect(&judge_ctx).await?;

    // 先登记本批已判定 ID + 扣减实际用量：即便无可注入消息，下批扫描也要越过
    // 这批头部；`剩余 -= 实际用量` 让后续挂点接着用同一份预算，不重复领取 5%。
    input.record_judged(&outcome.judged, outcome.used_tokens)?;

    let batch = outcome.injected;
    if batch.is_empty() {
        return Ok(match kind {
            PollKind::Front => NodeOutcome::new(NodeSignal::Done),
            PollKind::Exit => finish(),
        });
    }

    // 二选一判据：用户是否否定了当前意图。判断拿不到结果一律 supplement。
    // ⚠️ 与挂点位置无关 —— 图本身可回到任意节点，回 intent 时 intent_node
    // 会把旧结论 + 新输入组成 IntentRevision 做增量修订，不是从零重识别。
    let shift = input.judge_shift(&judge_ctx, &batch).await;
    if shift.needs_reinterpret() {
        input.advance_revision(batch)?;
        invalidate_downstream(bus);
        crate::log_info!(
            input.ctx(),
            "legacy_awaken",
            "poll {} reinterprets intent, agent_id={}, absorptions={}, available={}",
            match kind {
                PollKind::Front => "front",
                PollKind::Exit => "exit",
            },
            input.agent().po.id,
            absorptions + 1,
            available
        );
        return Ok(NodeOutcome::new(NodeSignal::Yield(
            SIGNAL_NEW_INPUT.to_string(),
        )));
    }

    // supplement：尾插追加，不回 intent、不作废检索与执行结果
    // 已过装配阶段（Exit）时 prepare 必须追加而非重建 base，否则 Phase 1 前缀与
    // assistant 意图结论会被重排丢进 Prompt 中部。
    input.advance_supplement(batch, kind == PollKind::Exit)?;
    let outcome = match kind {
        PollKind::Front => {
            crate::log_info!(
                input.ctx(),
                "legacy_awaken",
                "poll_front supplements current intent, agent_id={}, absorptions={}, available={}",
                input.agent().po.id,
                absorptions + 1,
                available
            );
            NodeOutcome::new(NodeSignal::Done)
        }
        PollKind::Exit => {
            // 只重装配 Prompt：检索结果仍然有效，不一起丢掉
            bus.invalidate(ContextKey::Prompt);
            crate::log_info!(
                input.ctx(),
                "legacy_awaken",
                "poll_exit supplements and rethinks, agent_id={}, absorptions={}",
                input.agent().po.id,
                absorptions + 1
            );
            NodeOutcome::new(NodeSignal::Yield(SIGNAL_RETHINK.to_string()))
        }
    };
    Ok(outcome)
}

/// 出口检查通过：用唯一信号收尾，避免与 `prepare` 的 `done` 入边竞争
fn finish() -> NodeOutcome {
    NodeOutcome::new(NodeSignal::Yield(SIGNAL_EXIT_DONE.to_string()))
}

/// 回退后让检索、装配与上一轮出口产物全部失效
///
/// `Artifacts` 必须一起清：从 `direct / clarify` 路径回退时，总线上还留着
/// 那次短路写下的直答/澄清文本；不清掉的话，一旦后续路径没有重新写产物，
/// 收尾节点会把已作废的答案原样发出去。
fn invalidate_downstream(bus: &mut ThinkBus) {
    bus.invalidate(ContextKey::Knowledge);
    bus.invalidate(ContextKey::Prompt);
    bus.invalidate(ContextKey::Artifacts);
}
