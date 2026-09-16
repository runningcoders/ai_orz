//! `agent.settle.requested` 的生产者：**只回答「重试到第几次就放弃」**
//!
//! ## 为什么需要一个「没有业务收尾」的生产者
//!
//! 消费者上报 `Err` 时，若框架按 `kind` 反查不到生产者，会走 `delivery_of` 兜底
//! → `Nack` **无限重投**（框架层不设 `max_retry`）。而 `agent.settle.requested`
//! 与 `message.created` 共用 `order_key = agent_id` 且都声明了 `ordered` ——
//! 一条一直失败（或长期 Busy）的沉淀请求会**永久占住该 Agent 的门闩**，
//! 让它后续的消息与结算全部饥饿。
//!
//! 沉淀链路自己维护 Agent 状态，本生产者**没有额外的业务状态要翻转**，
//! 所以 `on_consumed` 只留一条 debug 便于对账（与飞书入站生产者同构），
//! 真正的职责在 `on_failed` 的次数兜底。

use crate::pkg::RequestContext;
use crate::pkg::aop::{Producer, RetryDecision};
use crate::service::dal::inbound_retry;
use async_trait::async_trait;
use common::enums::EventTopic;
use common::error::Result;

/// 睡眠沉淀事件的生产者
pub struct AgentSettleProducer;

impl Default for AgentSettleProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentSettleProducer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Producer for AgentSettleProducer {
    fn name(&self) -> &str {
        "agent_settle"
    }

    fn topic(&self) -> EventTopic {
        EventTopic::AgentSettleRequested
    }

    /// 生命周期终结（成功消费 **或** 被放弃）→ 只留一条 debug
    ///
    /// 无状态可推进：Agent 的 Busy / Resting 与短期记忆的归档都由沉淀链路自己完成。
    /// ⚠️ **必须幂等**：回调先于 `queue.ack`，崩溃/重投时可能重复触发。
    async fn on_consumed(&self, _ctx: &RequestContext, event: &serde_json::Value) -> Result<()> {
        log_debug!(
            "[agent_settle] event lifecycle ended: event_id={} agent_id={}",
            event
                .get("event_id")
                .and_then(|v| v.as_str())
                .unwrap_or("<none>"),
            event
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("<none>")
        );
        Ok(())
    }

    /// 次数兜底：达到 [`inbound_retry::MAX_ATTEMPTS`] 或命中永久性错误 → `Discard`
    ///
    /// ⚠️ **绝不能无条件 `Retry`**：那会让一条失败的沉淀请求永久占住 `agent_id`
    /// 门闩（见模块文档）。`Discard` 时框架会打 error 日志 + 独立埋点，并仍回调
    /// 一次 [`Self::on_consumed`]，所以「放弃」是有痕迹、可审计的。
    ///
    /// ⚠️ 内部**不要**打 warn/error：`on_event` 失败处框架已打过 `sys_error!`，
    /// 这里再打一份就是重投风暴的第二份日志源（§4.3 日志纪律）。
    async fn on_failed(
        &self,
        _ctx: &RequestContext,
        _event: &serde_json::Value,
        err: &str,
        attempt: u32,
    ) -> Result<RetryDecision> {
        Ok(inbound_retry::decide(err, attempt))
    }
}
