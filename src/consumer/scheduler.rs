//! Cron 触发器消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 CRON_TRIGGER 事件。
//! 业务逻辑通过调用 domain 层完成（如 RuntimeAwakening.sleep_and_settle）。
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::handlers::hr::agent::settle_memory::load_and_settle;
use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use common::error::{Error, Result};

// ==================== 消费者实现 ====================

/// Cron 触发器消费者
///
/// 订阅 CRON_TRIGGER 事件，按 payload.action 分发到不同 domain 处理。
/// 作为 AOP 的 Sync 消费者，事件发布时直接调用 on_event。
pub struct CronTriggerConsumer;

impl Default for CronTriggerConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTriggerConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for CronTriggerConsumer {
    fn name(&self) -> &str {
        "cron_trigger"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("cron.trigger")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: CronTriggerEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize cron trigger event: {}", e))
        })?;

        sys_debug!(
            "received cron trigger event: {} (trigger_id: {}, action to be parsed)",
            event.id(),
            event.trigger_id
        );

        let payload: CronTriggerPayload = serde_json::from_str(&event.payload).map_err(|e| {
            Error::bad_request(format!(
                "invalid cron trigger payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "cron trigger fired: {} (trigger_id: {}, action: {})",
            event.trigger_name,
            event.trigger_id,
            payload.action
        );

        match payload.action.as_str() {
            "agent_rest" => {
                self.handle_agent_rest(&event, &payload.extra).await?;
            }
            _ => {
                sys_warn!(
                    "unknown action '{}' for trigger {} (id: {})",
                    payload.action,
                    event.trigger_name,
                    event.trigger_id
                );
            }
        }

        Ok(())
    }
}

// ==================== 业务编排（调用 domain 层）====================

impl CronTriggerConsumer {
    /// agent_rest 动作：加载 Agent 并调用 sleep_and_settle 执行记忆沉淀
    ///
    /// 复用 settle_memory handler 的 load_and_settle 公共函数，保证与神经工具触发的
    /// 沉淀流程完全一致（查询短期记忆 → 拼装 prompt → 加载 Agent → 唤醒 Brain → sleep_and_settle）。
    async fn handle_agent_rest(&self, event: &CronTriggerEvent, extra: &Value) -> Result<()> {
        let payload: AgentRestPayload = serde_json::from_value(extra.clone()).map_err(|e| {
            Error::bad_request(format!(
                "invalid agent_rest payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "agent_rest action triggered by {} (trigger_id: {}, agent_id: {})",
            event.trigger_name,
            event.trigger_id,
            payload.agent_id
        );

        let ctx = RequestContext::new_system();
        let settle_limit = payload.settle_limit.unwrap_or(10);

        let settled_count = load_and_settle(ctx, &payload.agent_id, settle_limit).await?;

        sys_info!(
            "agent {} settled {} short-term memories to knowledge nodes",
            payload.agent_id,
            settled_count
        );

        Ok(())
    }
}

// ==================== 辅助类型 ====================

#[derive(Debug, Serialize, Deserialize)]
struct CronTriggerPayload {
    action: String,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentRestPayload {
    agent_id: String,
    settle_limit: Option<usize>,
}
