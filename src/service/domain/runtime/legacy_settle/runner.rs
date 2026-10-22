//! `legacy_settle` 的运行入口：**`RuntimeAwakening::settle_pending` 的真实执行体**
//!
//! 与老 `sleep_and_settle` 的关系：**行为等价的重排**。Resting 状态、Trace、
//! 统计上报、AOP 事件的语义必须一一对齐 —— 这些都可观察，替换才有可对比性。
//!
//! ⚠️ 状态由本函数独占负责，调用方（`settle_agent_exclusive`）可能已经抢先
//! `try_set_resting`；这里再设一次是幂等的，且能覆盖「神经工具路径没有抢占」的场景。
//! 无论从哪条路径进来，`BusyGuard` 都保证结束/出错时回到 Idle。

use super::blueprint::legacy_settle_blueprint;
use super::context::{SettleAggregate, SettleDeps, build_providers};
use super::nodes::{FinalizeNode, NODE_FINALIZE, PrepareNode, SettleNode};
use crate::models::events::AgentLoopEvent;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::stats::AgentAwakeEvent;
use crate::pkg::think_path::{Arbiter, ThinkNode, ThinkPathEngine, ThinkRun};
use crate::service::domain::runtime::busy_guard::BusyGuard;
use common::error::Result;
use std::sync::Arc;

/// 一次 legacy_settle 的产出
#[derive(Debug)]
pub struct LegacySettleOutcome {
    pub agent_id: String,
    /// 真正沉淀（终态 `Final` 且成功标记）的条数
    pub settled_count: usize,
    /// 终态批次 / 模型调用批次 / 阻断原因
    pub batches: usize,
    pub attempts: usize,
    pub blocked: Option<String>,
    /// 走完的发生序列，如 `prepare#1>settle#1>prepare#2>finalize#1`
    pub fingerprint: String,
    pub steps: usize,
}

/// 跑一条 legacy_settle 路径
pub async fn run_legacy_settle(deps: Arc<SettleDeps>) -> Result<LegacySettleOutcome> {
    let agent_id = deps.agent.po.id.clone();

    crate::log_info!(
        &deps.ctx,
        "legacy_settle",
        "run_legacy_settle start, agent_id={}, budget_chars={}, item_cap={}, trace_ids={}",
        agent_id,
        deps.budget_chars,
        deps.item_cap(),
        deps.trace_ids.len()
    );

    // Resting 状态 + RAII 恢复 Idle（与老 `sleep_and_settle` 一致）
    AgentRuntimeStateManager::global().set_resting(&agent_id);
    let _rest_guard = BusyGuard::new(agent_id.clone());

    let blueprint = legacy_settle_blueprint()?;
    // ⚠️ 节点每次回环都要重新解析：`NodePool` 按 id 取 `Arc<dyn ThinkNode>`，
    // 三个节点实例在 run 内是共享的，状态全在 `deps` 的 Mutex 槽位里。
    let nodes: Vec<Arc<dyn ThinkNode>> = vec![
        Arc::new(PrepareNode::new(Arc::clone(&deps))),
        Arc::new(SettleNode::new(Arc::clone(&deps))),
        Arc::new(FinalizeNode::new(Arc::clone(&deps))),
    ];
    let engine = ThinkPathEngine::new(
        blueprint,
        nodes,
        Arbiter::deterministic(),
        build_providers(Arc::clone(&deps)),
    )?;

    let mut run = new_run(&deps);
    if let Err(e) = engine.run_into(&mut run).await {
        crate::log_error!(
            &deps.ctx,
            "legacy_settle",
            "engine run_into failed, agent_id={}, error={}",
            agent_id,
            e
        );
        return Err(finalize_error(deps, e).await);
    }
    if let Err(e) = ensure_finalized(&run) {
        return Err(finalize_error(deps, e).await);
    }
    Ok(collect(deps, &run))
}

/// 终止护栏：一条沉淀图必须**结束在收尾节点**
///
/// 与 `legacy_awaken::runner::ensure_consolidated` 同因：引擎把「没有合法候选」
/// 当正常结束，而回环图的预算耗尽正是这种情形。不识别就会安静地 `Ok` 返回：
/// 不写统计、不发循环完成事件，积压的记忆也没人知道。
fn ensure_finalized(run: &ThinkRun) -> Result<()> {
    let finished = run
        .trace
        .occurrences
        .last()
        .is_some_and(|last| last.def.as_str() == NODE_FINALIZE);
    if finished {
        return Ok(());
    }
    Err(common::error::err!(
        Internal,
        "legacy_settle terminated without reaching `{NODE_FINALIZE}`: run_id={} steps={} fingerprint={}",
        run.run_id,
        run.trace.steps,
        run.trace.fingerprint()
    ))
}

fn new_run(deps: &Arc<SettleDeps>) -> ThinkRun {
    ThinkRun {
        run_id: deps
            .run_anchor
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| format!("settle-{}", deps.started_at.elapsed().as_nanos())),
        blueprint: Arc::new(
            legacy_settle_blueprint().expect("legacy_settle blueprint must be valid"),
        ),
        stimulus: deps.stimulus(),
        bus: crate::pkg::think_path::ThinkBus::new(),
        trace: crate::pkg::think_path::RunTrace::default(),
        overlay: crate::pkg::think_path::StrategyOverlay::default(),
    }
}

/// 错误路径：统计 + 循环完成事件
///
/// 与 awaken 不同，**不做兜底存档**：沉淀没有用户对话可总结，写一条
/// `loop_abort` 短期记忆只会污染待沉淀队列（下次还会把它当输入再沉淀一遍）。
async fn finalize_error(
    deps: Arc<SettleDeps>,
    error: common::error::Error,
) -> common::error::Error {
    let agg = deps.aggregate.lock().map(|g| g.clone()).unwrap_or_default();
    let anchor = deps.run_anchor.lock().ok().and_then(|g| g.clone());
    let duration_ms = deps.started_at.elapsed().as_millis() as u64;
    let status = format!("settle failed: {error}");

    let _ = crate::record_event!(
        deps.ctx,
        AgentAwakeEvent {
            agent_id: deps.agent.po.id.clone(),
            project_id: None,
            task_id: None,
            organization_id: deps.ctx.organization_id.clone(),
            user_id: Some(deps.ctx.uid()),
            message_id: None,
            // 还没调过模型就失败（如取数失败）也记 1 次，便于与「空跑」区分
            call_count: agg.attempts.max(1) as u64,
            duration_ms: duration_ms,
            status: status.clone(),
            exit_reason: "error".to_string(),
        }
    );

    // 无锚点 = 还没取到第一批（空跑或 prepare 就失败）：与老行为一致不发 loop 事件
    if let Some(anchor) = anchor {
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
    }

    error
}

fn collect(deps: Arc<SettleDeps>, run: &ThinkRun) -> LegacySettleOutcome {
    let SettleAggregate {
        attempts,
        batches,
        memories,
        blocked,
    } = deps.aggregate.lock().map(|g| g.clone()).unwrap_or_default();

    LegacySettleOutcome {
        agent_id: deps.agent.po.id.clone(),
        settled_count: memories,
        batches,
        attempts,
        blocked,
        fingerprint: run.trace.fingerprint(),
        steps: run.trace.steps,
    }
}
