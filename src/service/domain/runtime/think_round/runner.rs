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
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::pkg::think_path::{Arbiter, ProviderSet, Stimulus, ThinkPathEngine, ThinkRun};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkLoopResult};
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
}

/// 装配并驱动轮级环图，返回循环终止结果
pub(crate) async fn run(domain: RuntimeDomainImpl, input: RoundRun<'_>) -> Result<ThinkLoopResult> {
    let RoundRun {
        ctx,
        agent,
        brain,
        scene,
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
    } = input;

    // 模型标识：事件上报用；与老循环「每轮共用同一份」的口径一致
    let (model_provider_id, model_name) = match brain.model_provider() {
        Some(po) => (Some(po.id.clone()), Some(po.model_name.clone())),
        None => (None, None),
    };

    // reflect 分支由 Agent 运行时开关控制；关闭时拓扑与老循环完全一致
    let enable_reflect = brain.runtime_config.enable_round_reflect;

    // 三层思考强度（0–100）：Provider 基准（能力开关）→ Agent 覆盖 → 场景偏移
    let thinking_intensity = resolve_thinking_intensity(&brain, scene);

    let deps = Arc::new(RoundDeps {
        domain,
        ctx,
        agent: agent.clone(),
        brain,
        scene,
        thinking_intensity,
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
    });

    let blueprint = blueprint::round_blueprint(available_rounds, enable_reflect)?;
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

/// 落盘一次轮级图轨迹（每 run 一行 JSONL）
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

/// 解析本次思考的最终思考强度（0–100 分）
///
/// 优先级：`Agent 显式 > Provider 基准`，再叠加场景偏移并饱和钳制到 [0, 100]。
///
/// ⚠️ Provider 未配置 `thinking_intensity` 时返回 None（能力开关关闭）：
/// 请求体完全不下发该字段，Agent 覆盖与场景偏移均不生效——保持存量行为是
/// 第一优先级，避免给不支持该参数的端点注入字段导致 400。
fn resolve_thinking_intensity(brain: &Brain, scene: ThinkingScene) -> Option<ThinkingIntensity> {
    let provider_base = brain
        .model_provider()
        .and_then(|po| po.config().thinking_intensity)?;
    let base = brain
        .runtime_config
        .thinking_intensity
        .unwrap_or(provider_base);
    Some(base.offset(scene.thinking_intensity_offset()))
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
mod tests {
    use super::resolve_thinking_intensity;
    use crate::models::agent::AgentRuntimeConfig;
    use crate::models::brain::Brain;
    use crate::models::model_provider::ModelProviderPo;
    use common::enums::{AgentKind, ThinkingIntensity, ThinkingScene};

    fn provider(config: &str) -> ModelProviderPo {
        ModelProviderPo {
            id: "prov".to_string(),
            name: "prov".to_string(),
            provider_type: common::enums::ProviderType::OpenAI,
            model_name: "gpt".to_string(),
            capability: common::enums::ModelCapability::Agent,
            api_key: String::new(),
            base_url: None,
            description: None,
            config: config.to_string(),
            status: common::enums::ModelProviderStatus::Normal,
            created_by: "system".to_string(),
            modified_by: "system".to_string(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn brain_with(config: &str, agent_intensity: Option<ThinkingIntensity>) -> Brain {
        Brain::new_local(
            "agent".to_string(),
            "agent".to_string(),
            AgentRuntimeConfig {
                thinking_intensity: agent_intensity,
                ..Default::default()
            },
            provider(config),
            Vec::new(),
        )
    }

    /// Provider 未配置 = 能力开关关闭：Agent 覆盖与场景偏移都不生效
    #[test]
    fn provider_unconfigured_disables_whole_chain() {
        let brain = brain_with("{}", Some(ThinkingIntensity::MAX));
        assert_eq!(
            resolve_thinking_intensity(&brain, ThinkingScene::Awaken),
            None
        );
    }

    /// 外部 Agent 无 provider → 同样关闭
    #[test]
    fn external_agent_without_provider_returns_none() {
        let brain = Brain::new_external(
            AgentKind::Cli,
            "agent".to_string(),
            "agent".to_string(),
            AgentRuntimeConfig::default(),
            Vec::new(),
        );
        assert_eq!(
            resolve_thinking_intensity(&brain, ThinkingScene::Awaken),
            None
        );
    }

    /// Agent 显式覆盖优先于 Provider 基准
    #[test]
    fn agent_override_wins_over_provider_default() {
        let brain = brain_with(r#"{"thinking_intensity":30}"#, Some(ThinkingIntensity::MAX));
        assert_eq!(
            resolve_thinking_intensity(&brain, ThinkingScene::Awaken),
            Some(ThinkingIntensity::MAX)
        );
    }

    /// 场景偏移在基准分数上叠加并饱和钳制到 [0, 100]
    #[test]
    fn scene_offset_clamps_at_bounds() {
        // 基准 10 + Settle(-20) → 钳制到 0
        assert_eq!(
            resolve_thinking_intensity(
                &brain_with(r#"{"thinking_intensity":10}"#, None),
                ThinkingScene::Settle
            ),
            Some(ThinkingIntensity::MIN)
        );
        // 基准 80 + IntentAnalyze(-20) → 60，跨档到 medium
        assert_eq!(
            resolve_thinking_intensity(
                &brain_with(r#"{"thinking_intensity":80}"#, None),
                ThinkingScene::IntentAnalyze
            ),
            Some(ThinkingIntensity::new(60))
        );
        // 默认基准 50 + Awaken(0) → 50
        assert_eq!(
            resolve_thinking_intensity(
                &brain_with(r#"{"thinking_intensity":50}"#, None),
                ThinkingScene::Awaken
            ),
            Some(ThinkingIntensity::DEFAULT)
        );
    }
}
