//! `legacy_awaken` 的运行入口：**`RuntimeAwakening::awaken` 的真实执行体**
//!
//! 与被它替换掉的那个平铺 `awaken` 的关系：**行为等价的重排**。忙碌状态、
//! Trace、压缩回环、统计上报、AOP 事件在这里必须一一对齐 —— 正因为这些语义
//! 可观察，替换才有可对比性，回滚也才有明确的落点。
//!
//! ⚠️ 影子模式下**不碰 Agent 运行时状态**（不 `set_busy`）、不落库、不发事件：
//! 影子跑的目的是对照，不能把排查过程变成新的副作用来源。

use super::blueprint::legacy_awaken_blueprint;
use super::context::{AwakenDeps, build_providers, read_bus};
use super::nodes::{
    Artifacts, BudgetState, CompactNode, ConsolidateNode, DeliberateNode, NODE_CONSOLIDATE,
    PrepareNode, PromptPack,
};
use crate::models::events::AgentLoopEvent;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::stats::AgentAwakeEvent;
use crate::pkg::think_path::{Arbiter, ContextKey, ThinkNode, ThinkPathEngine, ThinkRun};
use crate::service::domain::runtime::busy_guard::BusyGuard;
use common::error::Result;
use std::sync::Arc;

/// 一次 legacy_awaken 的产出（比 `AwakeningResult` 多一份新引擎的执行轨迹）
#[derive(Debug)]
pub struct LegacyAwakenOutcome {
    pub agent_id: String,
    pub trace_ids: Vec<String>,
    pub raw_input: String,
    pub raw_output: String,
    /// 走完的发生序列，如 `prepare#1>deliberate#1>consolidate#1`
    pub fingerprint: String,
    pub exit: String,
    pub steps: usize,
}

/// 跑一条 legacy_awaken 路径
pub async fn run_legacy_awaken(deps: Arc<AwakenDeps>) -> Result<LegacyAwakenOutcome> {
    let agent_id = deps.agent.po.id.clone();
    let message_id = deps.message.po.id.clone();

    // 诊断留痕：与 `wake_agent_brain` 的 "brain woken" 成对。若现场只有 "brain woken"
    // 而没有本行，说明冻结发生在 wake_brain 返回之后、进入图编排之前（例如 `awaken`
    // 构造 Trace / 装配依赖阶段），而不是思考循环内部。
    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "run_legacy_awaken start, agent_id={}, message_id={}, shadow={}",
        agent_id,
        message_id,
        deps.shadow
    );

    // 影子模式不动运行时状态：跑完整链路但不留副作用，目的只是对照
    let _busy_guard = if deps.shadow {
        None
    } else {
        AgentRuntimeStateManager::global().set_busy(
            &agent_id,
            &message_id,
            deps.message.po.task_id.as_deref(),
            deps.message.po.project_id.as_deref(),
        );
        let _ = crate::pkg::aop::publish(
            &deps.ctx,
            AgentLoopEvent::started(&agent_id, &deps.trace_id, "awaken", Some(&message_id)),
        )
        .await;
        Some(BusyGuard::new(agent_id.clone()))
    };

    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "busy set + loop started published, agent_id={}",
        agent_id
    );

    let blueprint = legacy_awaken_blueprint()?;
    let nodes: Vec<Arc<dyn ThinkNode>> = vec![
        Arc::new(PrepareNode::new(Arc::clone(&deps))),
        Arc::new(DeliberateNode::new(Arc::clone(&deps))),
        Arc::new(CompactNode::overflow(Arc::clone(&deps))),
        Arc::new(CompactNode::forced(Arc::clone(&deps))),
        Arc::new(ConsolidateNode::new(Arc::clone(&deps))),
    ];
    let engine = ThinkPathEngine::new(
        blueprint,
        nodes,
        Arbiter::deterministic(),
        build_providers(Arc::clone(&deps)),
    )?;

    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "engine built, entering run_into, agent_id={}",
        agent_id
    );

    let mut run = new_run(&deps);
    if let Err(e) = engine.run_into(&mut run).await {
        // 显式错误日志：此前本路径只发 AOP 事件、不打印 stderr，一次失败的唤醒
        // 在日志里"毫无痕迹"，极易被误判为"卡住没进展"。先把原因打到 stdout。
        crate::log_error!(
            &deps.ctx,
            "legacy_awaken",
            "engine run_into failed, agent_id={}, error={}",
            agent_id,
            e
        );
        return Err(finalize_error(deps, &run, e).await);
    }
    // 跑完不等于跑到底：见 [`ensure_consolidated`]
    if let Err(e) = ensure_consolidated(&run) {
        return Err(finalize_error(deps, &run, e).await);
    }
    Ok(collect(deps, &run))
}

/// 终止护栏：一条唤醒图必须**结束在收尾节点**
///
/// ⚠️ 这不是防御性编程。引擎把「没有任何合法候选」当作正常结束（`select`
/// 返回 `None` → `Ok(())`），而在有环的图里，**预算耗尽就是这种情形**。
/// 不去识别它的话，一次跑不完的唤醒会安静地 `Ok` 返回：
/// 不写 Trace、不报统计、不发循环完成事件、用户拿不到任何回复，
/// 日志里也没有一行报错 —— 正是老链路「Match 漏一个分支」那种静默失效。
///
/// 转成 `Err` 之后至少还有兜底存档（`finalize_error`）：进度快照会以
/// `loop_abort` 短期记忆落库，错误也照常冒泡给调用方决定重试与否。
fn ensure_consolidated(run: &ThinkRun) -> Result<()> {
    let finished = run
        .trace
        .occurrences
        .last()
        .is_some_and(|last| last.def.as_str() == NODE_CONSOLIDATE);
    if finished {
        return Ok(());
    }
    Err(common::error::err!(
        Internal,
        "legacy_awaken terminated without reaching `{NODE_CONSOLIDATE}`: run_id={} steps={} fingerprint={}",
        run.run_id,
        run.trace.steps,
        run.trace.fingerprint()
    ))
}

fn new_run(deps: &Arc<AwakenDeps>) -> ThinkRun {
    ThinkRun {
        run_id: deps.trace_id.clone(),
        blueprint: Arc::new(
            legacy_awaken_blueprint().expect("legacy_awaken blueprint must be valid"),
        ),
        stimulus: deps.stimulus(),
        bus: crate::pkg::think_path::ThinkBus::new(),
        trace: crate::pkg::think_path::RunTrace::default(),
        overlay: crate::pkg::think_path::StrategyOverlay::default(),
    }
}

/// 错误路径：规则化兜底存档（不依赖 LLM）+ 统计 + 事件，语义与老 awaken 一致
async fn finalize_error(
    deps: Arc<AwakenDeps>,
    run: &ThinkRun,
    error: common::error::Error,
) -> common::error::Error {
    if deps.shadow {
        return error;
    }
    let prompt = read_bus::<PromptPack>(&run.bus, ContextKey::Prompt)
        .map(|p| p.prompt)
        .unwrap_or_default();
    let duration_ms = deps.started_at.elapsed().as_millis() as u64;
    // 与老 `awaken` 同口径：送出的是**累计思考轮次**，不是步数。
    // 步数里包含 prepare / compact / consolidate，拿它当轮次会让存档里的
    // 「做了多少轮」虚高。
    let total_rounds = read_bus::<BudgetState>(&run.bus, ContextKey::Budget)
        .map(|b| b.total_rounds)
        .unwrap_or_default();

    let outcome = deps
        .domain
        .finalize_abort(super::super::abort_summary::AbortFinalizeParams {
            ctx: deps.ctx.clone(),
            agent: &deps.agent,
            trace: deps
                .trace
                .lock()
                .map(|t| t.clone())
                .unwrap_or_else(|poisoned| poisoned.into_inner().clone()),
            prompt: &prompt,
            user_message: deps.message.content(),
            message_id: &deps.message.po.id,
            progress: &deps.progress,
            total_rounds,
            trace_ids: std::slice::from_ref(&deps.trace_id),
            kind: super::super::abort_summary::AbortKind::Error,
            error: Some(&error),
        })
        .await;

    let status_code = format!("{:?}:{}", error.error_type, error.code());
    let reason = super::super::awakening::truncate_reason(
        &error.msg,
        super::super::awakening::AGENT_AWAKE_REASON_LIMIT,
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
            status: status_code,
            exit_reason: reason,
        }
    );
    let _ = crate::pkg::aop::publish(
        &deps.ctx,
        AgentLoopEvent::finished(
            &deps.agent.po.id,
            &deps.trace_id,
            "awaken",
            &format!("failed: {error}"),
            duration_ms,
            Some(&deps.message.po.id),
        ),
    )
    .await;

    super::super::abort_summary::attach_abort_notice(error, outcome.brief)
}

fn collect(deps: Arc<AwakenDeps>, run: &ThinkRun) -> LegacyAwakenOutcome {
    let prompt = read_bus::<PromptPack>(&run.bus, ContextKey::Prompt)
        .map(|p| p.prompt)
        .unwrap_or_default();
    let artifacts = read_bus::<Artifacts>(&run.bus, ContextKey::Artifacts).unwrap_or_default();
    LegacyAwakenOutcome {
        agent_id: deps.agent.po.id.clone(),
        trace_ids: vec![deps.trace_id.clone()],
        raw_input: prompt,
        raw_output: artifacts.content,
        fingerprint: run.trace.fingerprint(),
        exit: artifacts.exit,
        steps: run.trace.steps,
    }
}
