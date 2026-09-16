//! A2A 远端任务轮询「认领」事件
//!
//! 生产者（[`A2aPollingProducer`](crate::producer::a2a_polling::A2aPollingProducer)）
//! 每 30s 列出全部远端 Agent，**逐个 emit 一条本事件** —— 只做认领，绝不在
//! 轮询线程里碰网络 / DB 重活；真正的远端拉取、新消息投递、本地状态推进在
//! [`A2aPollConsumer`](crate::consumer::a2a_poll::A2aPollConsumer)（Async）里执行。
//!
//! 这是「Sync 消费者里的重量级动作只 publish、不在此线程同步执行」这条红线
//! 在 A2A 侧的落地（同 cron 触发器的 `agent_rest` 派发形态）。

use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

/// 远端 Agent 轮询认领事件
///
/// `order_key = agent_id` → 同一 Agent 的相邻两轮 tick 落在同一队列串行：
/// 上一轮没消费完时下一轮排队等待，不会重叠处理同一批任务。
/// （即使重叠也不致命：重复轮次由 task tags 里的 `a2a_synced_msgs` 计数幂等吸收，
/// 见 [`crate::models::events::get_synced_msg_count`]。）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aPollRequestedEvent {
    /// 事件标识（`agent_id + 轮次时间戳`；仅日志追踪与去重观测用）
    pub event_id: String,
    /// 目标远端 Agent ID
    pub agent_id: String,
    /// 事件创建时刻（毫秒）
    pub created_at: i64,
}

impl A2aPollRequestedEvent {
    /// 构造一次认领事件（`event_id` 带轮次时间戳，便于在日志里区分相邻两轮）
    pub fn new(agent_id: &str, tick_at: i64) -> Self {
        Self {
            event_id: format!("{}-{}", agent_id, tick_at),
            agent_id: agent_id.to_string(),
            created_at: tick_at,
        }
    }
}

impl Event for A2aPollRequestedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::A2aPollRequested
    }

    fn id(&self) -> &str {
        &self.event_id
    }

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

    #[test]
    fn event_id_and_order_key_are_agent_scoped() {
        let event = A2aPollRequestedEvent::new("agent-1", 1_772_000_000_000);
        assert_eq!(event.order_key(), "agent-1");
        assert_eq!(event.id(), "agent-1-1772000000000");
        assert_eq!(event.kind(), EventTopic::A2aPollRequested);
        assert_eq!(event.created_at(), 1_772_000_000_000);
    }
}
