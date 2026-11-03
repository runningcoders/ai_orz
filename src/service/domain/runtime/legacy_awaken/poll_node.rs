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
//! 三处共用 [`collect`] 收集器；动作差异：
//! - `poll_front`：尚未检索/执行 → 回意图做增量修订；
//! - `poll_exit`：已检索/执行过 → 只做尾插转向（`rethink`），不再回退；
//!   直答 / 澄清路径尚未检索 → 仍允许回意图修订。

use super::context::{AwakenDeps, read_bus};
use super::nodes::BudgetState;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::domain::runtime::inbox::{
    MAX_INJECTIONS_PER_WAKE, MAX_INTENT_REWINDS, collect,
};
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

/// 新输入收集节点
#[derive(Debug)]
pub struct PollNode {
    deps: Arc<AwakenDeps>,
    kind: PollKind,
}

impl PollNode {
    pub fn new(deps: Arc<AwakenDeps>, kind: PollKind) -> Self {
        Self { deps, kind }
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
        run_poll(&self.deps, self.kind, bus).await
    }
}

async fn run_poll(deps: &AwakenDeps, kind: PollKind, bus: &mut ThinkBus) -> Result<NodeOutcome> {
    // 剩余轮次：只剩最后一轮时不再注入，避免「听完就没轮次回答」
    let available = deps.options.effective_max_rounds().saturating_sub(
        read_bus::<BudgetState>(bus, ContextKey::Budget).map_or(0, |b| b.total_rounds),
    );
    let (anchor, cursor, injections, rewinds) = {
        let state = deps
            .inbox
            .lock()
            .map_err(|_| common::error::err!(Internal, "inbox state lock poisoned"))?;
        (
            state.anchor().clone(),
            state.cursor().clone(),
            state.injections(),
            state.rewinds(),
        )
    };

    let batch = collect(&deps.ctx, &anchor, &cursor).await?;
    if batch.is_empty() {
        return Ok(match kind {
            PollKind::Front => NodeOutcome::new(NodeSignal::Done),
            PollKind::Exit => finish(),
        });
    }

    if kind == PollKind::Front {
        // 轮次已耗尽：不再吸收，留给下一次独立唤醒（消息保持 Pending）
        if available < 1 {
            return Ok(NodeOutcome::new(NodeSignal::Done));
        }
        // 尚未产生副作用：能回退就回退，否则把新输入尾插进本轮 Prompt
        let can_rewind = rewinds < MAX_INTENT_REWINDS;
        let mut state = deps
            .inbox
            .lock()
            .map_err(|_| common::error::err!(Internal, "inbox state lock poisoned"))?;
        state.advance(&batch);
        if can_rewind {
            state.note_rewind();
            state.set_revision(batch);
            drop(state);
            invalidate_downstream(bus);
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "poll_front rewinds intent, agent_id={}, rewinds={}, available={}",
                deps.agent.po.id,
                rewinds + 1,
                available
            );
            return Ok(NodeOutcome::new(NodeSignal::Yield(
                SIGNAL_NEW_INPUT.to_string(),
            )));
        }
        state.set_pending_turn(batch);
        drop(state);
        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "poll_front absorbs without rewind, agent_id={}, rewinds={}, available={}",
            deps.agent.po.id,
            rewinds,
            available
        );
        return Ok(NodeOutcome::new(NodeSignal::Done));
    }

    // poll_exit：还剩最后一轮就不再折腾，让流程正常收尾
    if available < 1 {
        return Ok(finish());
    }

    // 已检索/执行过：新输入只做尾插转向，绝不把已完成工作全部作废
    let retrieved = bus.get(ContextKey::Knowledge).is_some();
    if !retrieved {
        if rewinds >= MAX_INTENT_REWINDS {
            return Ok(finish());
        }
        let mut state = deps
            .inbox
            .lock()
            .map_err(|_| common::error::err!(Internal, "inbox state lock poisoned"))?;
        state.advance(&batch);
        state.note_rewind();
        state.set_revision(batch);
        drop(state);
        invalidate_downstream(bus);
        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "poll_exit rewinds intent (pre-retrieve), agent_id={}, rewinds={}",
            deps.agent.po.id,
            rewinds + 1
        );
        return Ok(NodeOutcome::new(NodeSignal::Yield(
            SIGNAL_NEW_INPUT.to_string(),
        )));
    }

    if injections >= MAX_INJECTIONS_PER_WAKE {
        return Ok(finish());
    }
    let mut state = deps
        .inbox
        .lock()
        .map_err(|_| common::error::err!(Internal, "inbox state lock poisoned"))?;
    state.advance(&batch);
    state.note_injection();
    state.set_pending_turn(batch);
    // 这是唯一需要「在既有对话上续跑」的场景：prepare 必须追加而非重建 base，
    // 否则 Phase 1 前缀与 assistant 意图结论会被重排丢进 Prompt 中部
    state.mark_resume_append();
    drop(state);
    // 转向只重装配 Prompt：检索结果仍然有效，不能一起丢掉
    bus.invalidate(ContextKey::Prompt);
    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "poll_exit injects and rethinks, agent_id={}, injections={}",
        deps.agent.po.id,
        injections + 1
    );
    Ok(NodeOutcome::new(NodeSignal::Yield(
        SIGNAL_RETHINK.to_string(),
    )))
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
