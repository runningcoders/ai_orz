//! Think round stats consumer (AOP sync)
//!
//! 订阅 `agent.think.round` 事件，将每轮 think 的 token 用量
//! 记录到 `model_call_events` 表（ModelCallEvent）。
//!
//! 与 ToolExecStatsConsumer 类似，通过 global_stats() 访问 Stats 单例。

use async_trait::async_trait;

use crate::models::events::ThinkRoundEvent;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::pkg::stats::{ModelCallEvent, global_stats};
use common::error::Result;

pub struct ThinkRoundStatsConsumer;

impl ThinkRoundStatsConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ThinkRoundStatsConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for ThinkRoundStatsConsumer {
    fn name(&self) -> &str {
        "think_round_stats"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("agent.think.round")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: ThinkRoundEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!("failed to deserialize ThinkRoundEvent: {}", e))
        })?;

        // 跳过没有 token 用量的轮次（如外部 agent 无 model_provider）
        if event.total_tokens == 0 {
            return Ok(());
        }

        let stats_event = ModelCallEvent::new(event.created_at)
            .with_agent_id(Some(event.agent_id.clone()))
            .with_model_provider_id(event.model_provider_id.clone())
            .with_model_name(event.model_name.clone())
            .with_organization_id(event.organization_id.clone())
            .with_user_id(event.user_id.clone())
            .with_task_id(event.task_id.clone())
            .with_project_id(event.project_id.clone())
            .with_tokens_input(event.tokens_input)
            .with_tokens_output(event.tokens_output)
            .with_total_tokens(event.total_tokens);

        if let Some(stats) = global_stats() {
            let ctx = crate::pkg::request_context::RequestContext::new_system();
            let _ = stats.record(ctx, stats_event).await;
        }

        Ok(())
    }
}
