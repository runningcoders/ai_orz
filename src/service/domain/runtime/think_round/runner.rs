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
use crate::pkg::think_path::{Arbiter, ProviderSet, Stimulus, ThinkPathEngine};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkLoopResult};
use common::enums::ThinkingScene;
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

    let deps = Arc::new(RoundDeps {
        domain,
        ctx,
        agent: agent.clone(),
        brain,
        scene,
        trace_id: trace_id.to_string(),
        tool_descriptors: tool_descriptors.to_vec(),
        max_rounds,
        start_round,
        available_rounds,
        think_runtime,
        policy,
        progress,
        model_provider_id,
        model_name,
        state: Arc::new(Mutex::new(RoundState::new(messages))),
        loop_start,
    });

    let blueprint = blueprint::round_blueprint(available_rounds)?;
    let engine = ThinkPathEngine::new(
        blueprint,
        nodes::build_nodes(&deps),
        // 轮级环的小脑路由已前置在 run_think_loop，这里只做确定性推进
        Arbiter::deterministic(),
        ProviderSet::new(),
    )?;

    engine.run(trace_id, Stimulus::default()).await?;
    Ok(deps.take_result())
}
