//! Tool execution stats consumer (AOP sync)
//!
//! Replaces ToolCallLoggingDecorator's record_tool_call_stat.
//! Subscribes to "agent.tool.executed" events and records ToolCallEvent stats.

use async_trait::async_trait;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::models::events::ToolExecEvent;
use crate::pkg::stats::{ToolCallEvent, global_stats};
use crate::pkg::tool_tracing::entry::ToolCallStatus;
use common::error::Result;

pub struct ToolExecStatsConsumer;

impl ToolExecStatsConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecStatsConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for ToolExecStatsConsumer {
    fn name(&self) -> &str {
        "tool_exec_stats"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("agent.tool.executed")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: ToolExecEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!("failed to deserialize ToolExecEvent: {}", e))
        })?;

        let status = if matches!(event.entry.status, ToolCallStatus::Completed) {
            "success".to_string()
        } else {
            "failed".to_string()
        };

        let stats_event = ToolCallEvent::new(event.entry.finished_at as i64)
            .with_tool_id(event.entry.tool_id.clone())
            .with_tool_name(event.entry.tool_name.clone())
            .with_agent_id(event.entry.agent_id.clone())
            .with_project_id(event.entry.project_id.clone())
            .with_task_id(event.entry.task_id.clone())
            .with_organization_id(event.organization_id.clone())
            .with_user_id(event.user_id.clone())
            .with_args_len(event.args_len)
            .with_result_len(event.result_len)
            .with_duration_ms(event.entry.duration_ms)
            .with_status(status);

        if let Some(stats) = global_stats() {
            let ctx = crate::pkg::request_context::RequestContext::new_system();
            let _ = stats.record(ctx, stats_event).await;
        }

        Ok(())
    }
}
