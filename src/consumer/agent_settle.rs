//! Agent 沉淀消费者（AOP 异步）
//!
//! 承接 [`AgentSettleEvent`]（`agent.settle.requested`），逐个 Agent 执行睡眠沉淀。
//!
//! # 它解决的三个问题
//!
//! 1. **排队而不是丢弃**：`order_key = agent_id`（见事件定义）+ `try_set_resting` 原子抢占。
//!    抢不到说明 Agent 正被别的链路唤醒（典型：同一轮 cron poll 里项目巡检刚给它发了消息），
//!    此时返回冲突错误 → 队列 nack 重投 → 退避后自动重试，等它空闲再沉淀。
//!    旧实现在触发器里 `is_unavailable()` 判一下就 `Ok(0)` 静默跳过，而触发器已经把
//!    `next_run_at` 推到下一个 cron 点（日触发 = 次日），一次跳过等于丢一整天。
//! 2. **不阻塞调度**：触发器消费者是 `ConsumeMode::Sync`，直接在 cron `poll` 线程里跑一场
//!    沉淀（LLM 往返，实测数分钟）会把整个定时任务轮询堵住。
//! 3. **逐个隔离**：每个 Agent 一条事件、一次独立重试，一个 Agent 失败不再影响其余。
//!
//! # 可自愈（但不完全）
//!
//! 事件只在内存队列里，进程重启会丢。残留窗口 = **「派发后、沉淀跑完前」进程被杀**：
//! 触发器此时已 `mark_trigger_executed`，日触发的下个周期是次日，所以当天这个 Agent
//! 的沉淀不会自动重建（比旧实现「Agent 忙就跳过一次丢一天」窄得多）。
//!
//! 若要严格兜底，启动时对「仍有 Active 短期记忆的 Agent」补发一轮请求即可 ——
//! 沉淀对无待沉淀 Agent 是空跑，天然幂等。当前按 YAGNI 未做。

use async_trait::async_trait;
use common::error::{Error, ErrorCode, Result};

use crate::handlers::hr::agent::settle_memory::{SettleAttempt, settle_agent_exclusive};
use crate::models::events::AgentSettleEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};

/// Agent 沉淀消费者
pub struct AgentSettleConsumer;

impl Default for AgentSettleConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentSettleConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for AgentSettleConsumer {
    fn name(&self) -> &str {
        "agent_settle"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new(
            crate::models::events::agent_settle::AGENT_SETTLE_EVENT_KIND,
        )]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    /// 同时最多两场沉淀。沉淀一次要跑完整 LLM 往返（贵且长），
    /// 但只开 1 个 worker 会让「A 在等空闲、B 已经空闲」互相拖住。
    /// `order_key = agent_id` 已保证同 Agent 串行，这里只约束不同 Agent 的并行度。
    fn concurrency(&self) -> usize {
        2
    }

    fn empty_queue_sleep_ms(&self) -> u64 {
        200
    }

    /// 失败退避。主用途是「Agent 还在忙」的重排节奏：
    /// 沉淀是低频任务（日触发），30s 粒度完全够用，同时避免每 1s 刷一条框架级错误日志。
    fn error_retry_sleep_ms(&self) -> u64 {
        30_000
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: AgentSettleEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize agent settle event: {}", e))
        })?;

        // 先记一行「拿到请求」，与后面的冲突日志配对，便于从日志读出排队全过程
        log_info!(
            &ctx,
            "agent_settle",
            "agent_id={}, 收到沉淀请求（来源：{}）",
            event.agent_id,
            event.requested_by
        );

        match settle_agent_exclusive(ctx.clone(), &event.agent_id, event.settle_limit).await {
            Ok(SettleAttempt::Settled(count)) => {
                log_info!(
                    &ctx,
                    "agent_settle",
                    "agent_id={}, 沉淀完成，处理 {} 条短期记忆（来源：{}）",
                    event.agent_id,
                    count,
                    event.requested_by
                );
                Ok(())
            }
            Ok(SettleAttempt::Busy) => {
                // 不是错误，是「排队中」：Agent 正被消息链路唤醒（或已在沉淀）。
                // 上抛冲突让框架 nack 重投，退避后自动重试 —— 这就是沉淀的排队机制。
                Err(Error::conflict(format!(
                    "Agent {} 忙/休息中，沉淀请求重排（来源：{}）",
                    event.agent_id, event.requested_by
                )))
            }
            Err(e) if matches!(e.code_enum(), ErrorCode::ResourceNotFound) => {
                // Agent 已不存在 → 重试不可能成功，ack 丢弃避免每 30s 空转一次
                log_warn!(
                    &ctx,
                    "agent_settle",
                    "agent_id={}, Agent 不存在，沉淀请求作废（不再重试）: {}",
                    event.agent_id,
                    e
                );
                Ok(())
            }
            Err(e) => {
                // 模型/DB 等临时错误：交给框架 nack 重投（下次周期仍会兜底）
                log_warn!(
                    &ctx,
                    "agent_settle",
                    "agent_id={}, 沉淀失败，等待重试: {}",
                    event.agent_id,
                    e
                );
                Err(e)
            }
        }
    }
}
