//! 轮级环图的装配与驱动
//!
//! 把「一次思考循环」所需的全部依赖收敛成 [`RoundRun`]，装配蓝图 + 节点池后
//! 交给 `pkg::think_path` 引擎跑完，最后从共享状态取终止结果。
//!
//! 引擎不认识任何业务；节点之间也不互相引用——拓扑完全由 [`super::blueprint`] 决定。

use super::blueprint;
use super::deps::RoundDeps;
use super::nodes;
use super::state::RoundState;
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::models::cortex_types::{ChatMessage, ModelCallOptions, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::pkg::think_path::{Arbiter, ProviderSet, Stimulus, ThinkPathEngine, ThinkRun};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::thinking_strength::strength_base;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkLoopResult, WakeUsage};
use common::enums::{ThinkingIntensity, ThinkingScene};
use common::error::Result;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 一次轮级环图的运行入参
///
/// `messages` 已由调用方完成小脑路由注入；`agent` 为借用，装配时克隆一次，
/// 因为图节点必须 `'static`。
pub(crate) struct RoundRun<'a> {
    pub ctx: RequestContext,
    pub agent: &'a Agent,
    pub brain: Brain,
    pub scene: ThinkingScene,
    /// 意图识别后对后续阶段叠加的思考强度偏移（分）
    pub thinking_intensity_offset: i32,
    /// 小脑**已按配置基准折算**的思考强度（0–100 分；None = 走原逻辑）
    ///
    /// ⚠️ 不是凭空绝对分：`thinking_strength::select` 先取 `strength_base`、
    /// 再把小脑的相对偏移按剩余空间折算成这个值，管理员配的基准不会被覆盖。
    pub thinking_strength: Option<ThinkingIntensity>,
    /// 绝对思考强度（`Some` = 跳过 base/场景/动态偏移整条链；`None` = 逐字节等价）
    ///
    /// 唤醒门（`wake_gate`）判浅答时注入 `ThinkingIntensity::MIN`：强度链是加减
    /// 语义（`base=70` 时减不到 0），「浅答强度真 0」只能靠绝对通道表达。
    pub force_intensity: Option<ThinkingIntensity>,
    pub trace_id: &'a str,
    pub messages: Vec<ChatMessage>,
    pub tool_descriptors: &'a [ToolDescriptor],
    pub max_rounds: usize,
    pub start_round: usize,
    pub available_rounds: usize,
    pub think_runtime: Option<Arc<AgentThinkRuntime>>,
    pub policy: Option<Arc<dyn Policy>>,
    pub progress: Option<ThinkLoopProgress>,
    pub loop_start: Instant,
    /// 唤醒级用量表：本周期每轮要把累计值写回（跨周期预算的唯一载体）
    pub usage: Arc<Mutex<WakeUsage>>,
    /// 进入本周期前的已用量（本次循环的计量起点）
    pub base: WakeUsage,
    /// 唤醒级收件箱（仅 awaken 主链为 Some）
    pub inbox: Option<Arc<Mutex<crate::service::domain::runtime::inbox::InboxState>>>,
}

/// 装配并驱动轮级环图，返回循环终止结果
pub(crate) async fn run(domain: RuntimeDomainImpl, input: RoundRun<'_>) -> Result<ThinkLoopResult> {
    let RoundRun {
        ctx,
        agent,
        brain,
        scene,
        thinking_intensity_offset,
        thinking_strength,
        force_intensity,
        trace_id,
        messages,
        tool_descriptors,
        max_rounds,
        start_round,
        available_rounds,
        think_runtime,
        policy,
        progress,
        loop_start,
        usage,
        base,
        inbox,
    } = input;

    // 模型标识：事件上报用；与老循环「每轮共用同一份」的口径一致
    let (model_provider_id, model_name) = match brain.model_provider() {
        Some(po) => (Some(po.id.clone()), Some(po.model_name.clone())),
        None => (None, None),
    };

    // reflect 分支由 Agent 运行时开关控制；关闭时拓扑与老循环完全一致
    let enable_reflect = brain.runtime_config.enable_round_reflect;

    // 思考强度（0–100）：配置基准 → 小脑相对偏移折算 → 场景偏移 → 动态偏移。
    // 配置基准不再被小脑绝对分替换，见 `thinking_strength::strength_base`。
    let call_options = resolve_call_options(
        &brain,
        scene,
        thinking_intensity_offset,
        thinking_strength,
        force_intensity,
    );

    let deps = Arc::new(RoundDeps {
        domain,
        ctx,
        agent: agent.clone(),
        brain,
        scene,
        call_options,
        trace_id: trace_id.to_string(),
        tool_descriptors: tool_descriptors.to_vec(),
        max_rounds,
        start_round,
        available_rounds,
        think_runtime,
        policy,
        progress,
        enable_reflect,
        model_provider_id,
        model_name,
        state: Arc::new(Mutex::new(RoundState::new(messages))),
        loop_start,
        usage,
        base,
        inbox,
    });

    let poll_enabled = deps.inbox.is_some();
    let blueprint =
        blueprint::round_blueprint_with_poll(available_rounds, enable_reflect, poll_enabled)?;
    let engine = ThinkPathEngine::new(
        blueprint,
        nodes::build_nodes(&deps),
        // 轮级环的小脑路由已前置在 run_think_loop，这里只做确定性推进
        Arbiter::deterministic(),
        ProviderSet::new(),
    )?;

    let run = engine.run(trace_id, Stimulus::default()).await?;
    persist_trace(&deps, &run);
    Ok(deps.take_result())
}

/// 追加一条「运行中」的轮级增量轨迹（每轮定型后调用）
///
/// 与 [`persist_trace`] 的分工：本函数只写**单轮**明细（`partial=true`），让前端在
/// run 未结束时查询也能看到已完成的轮次；run 收尾的 [`persist_trace`] 再写完整行
/// （`partial=false`，含 occurrences / routes / fingerprint）。
///
/// 查询侧按同一 `(agent_id, run_id, scene)` 合并，收尾行会**替换本片段**这里累积的
/// rounds（同一批数据的两个粒度，拼接会重复），片段之间仍拼接续编号；
/// 因此两条路径可以安全并存，多次重跑也不会互相冲掉轮次。
/// 写失败与收尾写盘同口径：只告警，不影响思考主流程。
///
/// ⚠️ 这里刻意**同步**写盘，不要改成 `spawn_blocking` / 后台队列：JSONL 是
/// append-only，行序即轮次序，而 `merge_into` 的「增量行累积 → 收尾行替换」完全
/// 依赖这个顺序。一旦并发写导致后一轮先落盘，合并出的轨迹轮次就会错序。
/// 单轮一次小写入的阻塞代价，远小于轨迹错乱的排障代价。
pub(super) fn append_partial_round(
    deps: &RoundDeps,
    round: &crate::pkg::think_trace::ThinkTraceRound,
) {
    let entry = serde_json::json!({
        "run_id": deps.trace_id,
        "agent_id": deps.agent.po.id,
        "scene": deps.scene.as_str(),
        "project_id": deps.ctx.project_id(),
        "task_id": deps.ctx.task_id(),
        // 与 persist_trace 的 run.blueprint.id 同源（常量），改蓝图 ID 时两边一起动
        "path": super::blueprint::THINK_ROUND_BLUEPRINT_ID,
        // 增量行不含图执行信息，全部由收尾行提供
        "steps": 0,
        "advisor_calls": 0,
        "fingerprint": "",
        "partial": true,
        "rounds": [round],
        "occurrences": [],
        "routes": [],
    });
    if let Err(e) = crate::pkg::think_trace::append(&deps.agent.po.id, &entry) {
        crate::log_warn!("think round partial trace write failed: {}", e);
    }
}

/// 落盘一次轮级图轨迹的收尾行（`partial=false`）
///
/// 一轮思考循环 = 每轮各一行增量行（[`append_partial_round`]）+ 收尾这里一行
/// 完整行；查询侧合并成一条连续轨迹。
///
/// 动态分叉（reflect）出问题时，只有这里有完整的 occurrence 序列与选路来源；
/// 写失败不影响思考结果，仅告警——可观测性不能反过来拖垮主流程。
fn persist_trace(deps: &RoundDeps, run: &ThinkRun) {
    // 轮级明细由各节点写入共享状态；锁中毒时仍尽量取出已采集的部分，
    // 轨迹是排障数据，宁可残缺也不能整条丢失
    let rounds = match deps.state.lock() {
        Ok(state) => state.trace_rounds.clone(),
        Err(poisoned) => poisoned.into_inner().trace_rounds.clone(),
    };
    let entry = serde_json::json!({
        "run_id": run.run_id,
        "agent_id": deps.agent.po.id,
        "scene": deps.scene.as_str(),
        // 项目/任务维度：对话页右侧「思考轨迹」按项目聚合多 Agent 时用
        "project_id": deps.ctx.project_id(),
        "task_id": deps.ctx.task_id(),
        "path": run.blueprint.id,
        "partial": false,
        // run 级小脑留痕：route / 思考强度 / 两路精选在此一次性取走并清空
        "cerebellum": crate::pkg::think_trace::take_cerebellum(&deps.trace_id),
        "steps": run.trace.steps,
        "advisor_calls": run.trace.advisor_calls,
        "fingerprint": run.trace.fingerprint(),
        "rounds": rounds,
        "occurrences": run
            .trace
            .occurrences
            .iter()
            .map(|o| serde_json::json!({
                "node": o.def.as_str(),
                "seq": o.seq,
                "step": o.step_index,
                "signal": o.signal.as_ref().map(|s| s.tag().to_string()),
            }))
            .collect::<Vec<_>>(),
        "routes": run
            .trace
            .routes
            .iter()
            .map(|r| serde_json::json!({
                "step": r.step,
                "candidates": r.candidates.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
                "chosen": r.chosen.as_str(),
                "via": via_str(r.via),
                "reason": r.reason,
            }))
            .collect::<Vec<_>>(),
    });
    if let Err(e) = crate::pkg::think_trace::append(&deps.agent.po.id, &entry) {
        crate::log_warn!("think round trace write failed: {}", e);
    }
}

/// 解析本次思考的模型调用参数（当前只有思考强度）
///
/// 基准优先级：`小脑折算值 > 配置基准（Agent 显式 > Provider 基准）`，再叠加
/// 「场景偏移 + 意图动态偏移」并饱和钳制到 [0, 100]。
///
/// ⚠️ 小脑值**不是凭空绝对分**：`thinking_strength::select` 已经把它的相对
/// 偏移按配置基准的剩余空间折算成 0–100（见 `strength_base`）。所以这里的
/// 「小脑优先」是「以配置基准为锚、加减后仍优先」，不是覆盖管理员配置。
/// 场景偏移仍照旧叠加——场景是机制层的固有差异（收尾总结要稳、压缩要省），
/// 不该被模型的一次判断抹平。
///
/// 2026-11-12 事故：此前小脑直接给绝对分并排在第一位，实测 73 次判定全部
/// 落在 19–58（中位 46），把显式配置的 100 静默替换成 medium。
///
/// ⚠️ Provider 未配置 `thinking_intensity` 时返回 None（能力开关关闭）：
/// 请求体完全不下发该字段，小脑选定值与 Agent 覆盖均不生效——保持存量行为是
/// 第一优先级，避免给不支持该参数的端点注入字段导致 400。此时小脑的分数仍会
/// 写进思考轨迹（`applied=false`），不会静默消失。
fn resolve_call_options(
    brain: &Brain,
    scene: ThinkingScene,
    dynamic_offset: i32,
    cerebellum_strength: Option<ThinkingIntensity>,
    force_intensity: Option<ThinkingIntensity>,
) -> ModelCallOptions {
    let thinking_intensity = brain
        .model_provider()
        .and_then(|po| po.config().thinking_intensity)
        .map(|_provider_base| {
            // 绝对强度（唤醒门判浅答）：跳过 `base + 场景偏移 + 动态偏移` 整条链。
            //
            // 必须在 Provider 能力开关的 map 闭包**内部**判断：Provider 未配
            // `thinking_intensity` 时整链不下发（`None`），本通道也不得绕过能力
            // 开关往不支持的端点注入字段。
            //
            // 不复用 `cerebellum_strength`：它只替换基准、之后仍叠加偏移；强度链
            // 是加减语义，`base=70` 加偏移减不到 0，「浅答强度设为 0」是假语义。
            if let Some(forced) = force_intensity {
                return forced;
            }
            // `cerebellum_strength` 已是「配置基准 + 相对偏移」折算后的绝对值；
            // 降级（None）时才由 `strength_base` 直接给配置基准。能力开关仍在
            // 外层 map：provider 未配 `thinking_intensity` 时整链不下发。
            let base = cerebellum_strength.unwrap_or_else(|| strength_base(brain));
            base.offset(scene.thinking_intensity_offset() + dynamic_offset)
        });
    ModelCallOptions { thinking_intensity }
}

/// 选路来源的可读名（落盘后按字符串聚合，避免 Debug 输出随枚举改名漂移）
fn via_str(via: crate::pkg::think_path::RouteVia) -> &'static str {
    use crate::pkg::think_path::RouteVia;
    match via {
        RouteVia::Unique => "unique",
        RouteVia::Deterministic => "deterministic",
        RouteVia::Advisor => "advisor",
        RouteVia::Fallback => "fallback",
    }
}
#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
