//! Tool execution log consumer (AOP sync)
//!
//! Replaces ToolCallLoggingDecorator's JSONL logging.
//! Subscribes to "agent.tool.executed" events and writes to ToolCallLogger.

use crate::models::events::ToolExecEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use async_trait::async_trait;
use common::enums::EventTopic;
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

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::AgentToolExecuted)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: ToolExecEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!("failed to deserialize ToolExecEvent: {}", e))
        })?;

        // 写入 JSONL 日志（与原 decorator 的 log_call 逻辑一致）
        // 路径为 tools/call_trace/{YYYYMMDD}.jsonl —— 不按 tool_id 分目录，
        // tool_id 只作为 entry 字段落盘（见 paths::tool_call_trace_dir 的边界决策）。
        let logger = ToolCallLogger::get();
        let _ = logger.log_call(event.entry);

        Ok(())
    }
}
