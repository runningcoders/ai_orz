//! `legacy_awaken` 的运行入口（尚未接入调用方，供影子对比调用）
//!
//! 与老 `awaken` 的关系：**行为等价的重排**。忙碌状态、Trace、统计、AOP 事件
//! 都能观察到 —— 所以它可以先于「替换」存在，用于同消息对比。
//!
//! ⚠️ 影子模式下**不碰 Agent 运行时状态**（不 `set_busy`）、不落库、不发事件：
//! 影子跑的目的是对照，不能把排查过程变成新的副作用来源。

use super::blueprint::legacy_awaken_blueprint;
use super::context::{AwakenDeps, build_providers, read_bus};
use super::nodes::{
    Artifacts, CompactNode, ConsolidateNode, DeliberateNode, PrepareNode, PromptPack,
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

    let mut run = new_run(&deps);
    if let Err(e) = engine.run_into(&mut run).await {
        return Err(finalize_error(deps, &run, e).await);
    }
    Ok(collect(deps, &run))
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
            total_rounds: run.trace.steps,
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

/// 影子对比：与新链路共用入参，产出可直接比对的执行摘要
pub async fn shadow_run(deps: Arc<AwakenDeps>) -> ShadowReport {
    let started = std::time::Instant::now();
    match run_legacy_awaken(deps).await {
        Ok(outcome) => ShadowReport {
            ok: true,
            duration_ms: started.elapsed().as_millis() as u64,
            fingerprint: outcome.fingerprint,
            exit: outcome.exit,
            steps: outcome.steps,
            reply_len: outcome.raw_output.chars().count(),
            error: None,
        },
        Err(e) => ShadowReport {
            ok: false,
            duration_ms: started.elapsed().as_millis() as u64,
            fingerprint: String::new(),
            exit: "error".to_string(),
            steps: 0,
            reply_len: 0,
            error: Some(format!("{e}")),
        },
    }
}

/// 影子对比报告：与老链路同口径（耗时 / 指纹 / 退出原因 / 是否有产出）
#[derive(Debug, Clone, Default)]
pub struct ShadowReport {
    pub ok: bool,
    pub duration_ms: u64,
    pub fingerprint: String,
    pub exit: String,
    pub steps: usize,
    pub reply_len: usize,
    pub error: Option<String>,
}
