//! 轮级环图节点的共享依赖
//!
//! 节点必须是 `'static`（引擎持有 `Arc<dyn ThinkNode>`），所以这里全部持所有权：
//! - `RuntimeDomainImpl` / `Agent` / `Brain` 都是 `Clone`（内部 Arc / 轻量字段）
//! - 策略组与运行时快照改为 `Arc`，供三个节点共享同一份
//! - `messages` 等可变状态在 [`RoundState`] 中，经 `Arc<Mutex<_>>` 共享

use super::state::RoundState;
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::models::cortex_types::{ModelCallOptions, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkLoopResult, WakeUsage};
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
    /// 本次思考的模型调用参数（思考强度已按三层规则解析，能力开关关闭时为 None）
    pub call_options: ModelCallOptions,
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
    /// 是否启用 reflect 分支（Agent 运行时开关）
    pub enable_reflect: bool,
    /// 事件上报用的模型标识
    pub model_provider_id: Option<String>,
    pub model_name: Option<String>,
    pub state: Arc<Mutex<RoundState>>,
    /// 策略 `elapsed_secs` 的起点
    pub loop_start: Instant,
    /// 唤醒级用量表（每轮写回；跨周期的唯一预算载体）
    pub usage: Arc<Mutex<WakeUsage>>,
    /// 进入本周期前的已用量（本次循环的计量起点）
    pub base: WakeUsage,
}

/// 唤醒级轮次编号（1 基）= 进入本周期前已消耗的轮次 + 本周期内局部轮次
///
/// ⚠️ 绝不是 `local_round + 1`：`state.last_round` 是**本周期内**的 0 基索引，
/// 每个周期都从 0 重新数。少了 `start_round` 会让跨周期累计彻底失效——
/// 策略的 `round_number >= max_rounds` 在第二个周期永远撞不到（报 1..4 而非
/// 7..10），退出会退化成撞节点激活预算耗尽；写回计量表的 `rounds` 也变小，
/// 下一周期又领回一大截额度。轨迹轮次 / 事件编号 / 策略入参必须同源走这里。
pub(crate) fn wake_round_number(start_round: usize, local_round: usize) -> usize {
    start_round + local_round + 1
}

impl RoundDeps {
    /// 当前**唤醒级**轮次编号（1 基）
    pub(crate) fn wake_rounds(&self, local_round: usize) -> usize {
        wake_round_number(self.start_round, local_round)
    }

    /// 同一口径的 0 基版本（事件 / 轨迹内部按 0 基 index 传递的地方用它）
    pub(crate) fn wake_round_index(&self, local_round: usize) -> usize {
        self.start_round + local_round
    }

    /// 当前**唤醒级**已消耗的 token（本次循环累计 + 进入前的基数）
    pub(crate) fn wake_tokens(&self, local_tokens: u64) -> u64 {
        self.base.tokens.saturating_add(local_tokens)
    }

    /// 当前**唤醒级**已消耗的墙钟秒数
    pub(crate) fn wake_elapsed_secs(&self) -> u64 {
        self.base
            .elapsed_secs
            .saturating_add(self.loop_start.elapsed().as_secs())
    }

    /// 把当前累计用量写回计量表（下一周期从这里接着算）
    pub(crate) fn sync_usage(&self, round_1based: usize, local_tokens: u64) {
        WakeUsage::store(
            &self.usage,
            WakeUsage {
                rounds: round_1based,
                tokens: self.wake_tokens(local_tokens),
                elapsed_secs: self.wake_elapsed_secs(),
            },
        );
    }
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

#[cfg(test)]
mod tests {
    use super::wake_round_number;

    /// 唤醒级轮次编号必须带上 `start_round`
    ///
    /// ⚠️ 这条盯的是一个**已经真实发生过**的缺陷：写成 `local_round + 1` 时，
    /// 第二个思考周期报的 `round_number` 是 1..4 而不是 7..10，于是
    /// `MaxRoundsPolicy`（`round_number >= max_rounds`）永远撞不到上限，
    /// 退出退化成撞节点激活预算耗尽——「配了 10 轮却在第 11 轮还在思考」
    /// 就是从这里来的。它不报错、不 panic，只是预算静默失效。
    #[test]
    fn wake_round_number_includes_start_round() {
        // 第一个周期（start_round = 0）：局部即绝对
        assert_eq!(wake_round_number(0, 0), 1);
        assert_eq!(wake_round_number(0, 9), 10);

        // 第二个周期：已从第 10 轮接着跑，局部第 0 轮 = 绝对第 11 轮
        assert_eq!(wake_round_number(10, 0), 11);
        assert_eq!(wake_round_number(6, 3), 10);
    }

    /// 总预算 10、起点第 7 轮 ⇒ 局部跑满 3 轮时绝对轮次正好撞上上限
    ///
    /// 这是策略能正常触发 `MaxRoundsExceeded` 的前提：如果编号少了
    /// `start_round`，这里会算成 4，`4 >= 10` 永远为假。
    #[test]
    fn wake_round_number_reaches_the_cap_within_the_cycle() {
        let max_rounds = 10usize;
        let start_round = 6usize;
        let available = max_rounds.saturating_sub(start_round);
        let last_round = wake_round_number(start_round, available - 1);
        assert_eq!(last_round, 10);
        assert!(last_round >= max_rounds, "最后一轮必须够到策略判定的上限");
    }
}
