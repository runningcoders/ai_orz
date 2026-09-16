//! Agent 沉淀请求事件（AOP 异步）
//!
//! 定时触发器（`cron.trigger` 的 `agent_rest`）**只负责派发**，真正的一次沉淀由本事件
//! 的消费者承担 —— 即 `consumer/message.rs` 的 `agent.awakening` 消费者
//! （与 `message.created` 同一个消费者，见该模块文档说明为何必须合并）。
//!
//! # 为什么要落成独立事件而不是在触发器里直接调
//!
//! 1. **不阻塞调度**：触发器消费者是 `ConsumeMode::Sync`，直接在 `poll` 线程里跑一场
//!    沉淀（LLM 往返，实测数分钟）会把整个 cron 轮询堵住。
//! 2. **不丢失**：`order_key = agent_id` 让沉淀与发给同一 Agent 的消息落在同一条队列上
//!    串行 —— 沉淀在跑时消息压根不出队（不失败、不重试、不刷日志），沉淀 `ack` 后队列
//!    才推进。旧实现是触发器里 `is_unavailable()` 判一下就静默跳过，而触发器已经把
//!    `next_run_at` 推到下一个 cron 点（日触发 = 次日），一次跳过等于丢一天。

use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// 事件类型（AOP 路由 key）
pub const AGENT_SETTLE_EVENT_KIND: &str = "agent.settle.requested";

/// Agent 沉淀请求：睡眠沉淀的排队单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettleEvent {
    pub event_id: String,
    /// 目标 Agent
    pub agent_id: String,
    /// 本批待沉淀条数上限（调用方按需压小，实际批量由上下文预算决定）
    pub settle_limit: usize,
    /// 触发来源名称，仅用于日志溯源（如「系统默认-Agent 睡眠沉淀」）
    pub requested_by: String,
    pub created_at: i64,
}

impl AgentSettleEvent {
    pub fn new(agent_id: &str, settle_limit: usize, requested_by: &str) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            settle_limit,
            requested_by: requested_by.to_string(),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for AgentSettleEvent {
    fn kind(&self) -> EventKind {
        EventKind::new(AGENT_SETTLE_EVENT_KIND)
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    /// 同一 Agent 串行：与 `message.created` 对 Agent 接收者的 order_key 取法一致，
    /// 保证沉淀不会与发给同一个 Agent 的消息并发唤醒它。
    fn order_key(&self) -> &str {
        &self.agent_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁定 order_key = agent_id：这是「同 Agent 串行 + 忙时排队重试」的支点，
    /// 改成常量或事件 id 会让同一 Agent 的沉淀与消息并发唤醒它。
    #[test]
    fn test_order_key_is_agent_id() {
        let event = AgentSettleEvent::new("agent-001", 10, "系统默认-Agent 睡眠沉淀");
        assert_eq!(event.kind().0, AGENT_SETTLE_EVENT_KIND);
        assert_eq!(event.order_key(), "agent-001");
        assert_eq!(event.id(), event.event_id.as_str());
        assert_eq!(event.settle_limit, 10);
        assert_eq!(event.requested_by, "系统默认-Agent 睡眠沉淀");
        assert!(event.created_at > 0);
    }
}
