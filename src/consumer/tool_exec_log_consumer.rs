//! Tool execution log consumer (AOP sync)
//!
//! Replaces ToolCallLoggingDecorator's JSONL logging.
//! Subscribes to "agent.tool.executed" events and writes to ToolCallLogger.

use async_trait::async_trait;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::models::events::ToolExecEvent;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use common::error::Result;

pub struct ToolExecLogConsumer;

impl ToolExecLogConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecLogConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for ToolExecLogConsumer {
    fn name(&self) -> &str {
        "tool_exec_log"
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

        // 写入 JSONL 日志（与原 decorator 的 log_call 逻辑一致）
        let logger = ToolCallLogger::get();
        let tool_id = event.entry.tool_id.clone();
        let _ = logger.log_call(&tool_id, event.entry);

        Ok(())
    }
}
