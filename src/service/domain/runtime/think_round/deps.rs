//! 轮级环图节点的共享依赖
//!
//! 节点必须是 `'static`（引擎持有 `Arc<dyn ThinkNode>`），所以这里全部持所有权：
//! - `RuntimeDomainImpl` / `Agent` / `Brain` 都是 `Clone`（内部 Arc / 轻量字段）
//! - 策略组与运行时快照改为 `Arc`，供三个节点共享同一份
//! - `messages` 等可变状态在 [`RoundState`] 中，经 `Arc<Mutex<_>>` 共享

use super::state::RoundState;
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::models::cortex_types::ToolDescriptor;
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkLoopResult};
use common::enums::ThinkingScene;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 一次轮级环图的全部运行依赖
pub(crate) struct RoundDeps {
    /// 业务入口（思考 / 工具执行都经它转发）
    pub domain: RuntimeDomainImpl,
    pub ctx: RequestContext,
    pub agent: Agent,
    /// 已唤醒的大脑（思考调用直接持有，避免每轮再判空）
    pub brain: Brain,
    pub scene: ThinkingScene,
    pub trace_id: String,
    pub tool_descriptors: Vec<ToolDescriptor>,
    /// 总轮次上限（跨压缩累计）
    pub max_rounds: usize,
    /// 本次循环起始轮次（跨压缩累计）
    pub start_round: usize,
    /// 本次循环可用轮次（= max_rounds - start_round，同时作为节点激活预算）
    pub available_rounds: usize,
    pub think_runtime: Option<Arc<AgentThinkRuntime>>,
    pub policy: Option<Arc<dyn Policy>>,
    pub progress: Option<ThinkLoopProgress>,
    /// 事件上报用的模型标识
    pub model_provider_id: Option<String>,
    pub model_name: Option<String>,
    pub state: Arc<Mutex<RoundState>>,
    /// 策略 `elapsed_secs` 的起点
    pub loop_start: Instant,
}

impl std::fmt::Debug for RoundDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoundDeps")
            .field("agent_id", &self.agent.po.id)
            .field("scene", &self.scene.as_str())
            .field("trace_id", &self.trace_id)
            .field("available_rounds", &self.available_rounds)
            .finish()
    }
}

impl RoundDeps {
    /// 取走终止结果
    ///
    /// 图自然终止（无合法候选）时 `state.result` 可能为空——按老循环语义
    /// 归为「轮次耗尽」：可用轮次全部用完仍未得到 Final。
    pub(crate) fn take_result(&self) -> ThinkLoopResult {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            // 锁中毒时状态仍可读：宁可返回一份可能不完整的结果，也不让循环静默失败
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .result
            .take()
            .unwrap_or_else(|| ThinkLoopResult::MaxRoundsExceeded {
                messages: std::mem::take(&mut state.messages),
                total_rounds: self.max_rounds,
            })
    }
}
