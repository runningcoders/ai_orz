//! Logging decorator for our core Tool trait (with explicit RequestContext)
//! Logging decorator for our core Tool trait (with explicit RequestContext)
//! Wraps tools that are called through our manual built-in call chain
//! to automatically log invocations the same way.

use common::error::Result;
use async_trait::async_trait;
use common::enums::ToolProtocol;
use serde_json::Value;
use uuid::Uuid;
use std::sync::Arc;

use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::ToolCallEvent;
use common::constants::utils::current_timestamp_ms;

/// Logging decorator that wraps a Tool instance and automatically logs all calls
#[derive(Clone)]
pub struct LoggingDecorator {
    /// The inner tool that actually does the work
    inner: Box<dyn CoreTool + Send + Sync>,
    /// Logger instance for storing call traces
    logger: Arc<ToolCallLogger>,
}

impl LoggingDecorator {
    /// Create a new logging decorator wrapping an existing tool
    /// Uses the global ToolCallLogger singleton
    pub fn new(inner: Box<dyn CoreTool + Send + Sync>) -> Self {
        Self {
            inner,
            logger: Arc::new(ToolCallLogger::get().clone()),
        }
    }

    /// Create a new logging decorator with a custom logger instance
    /// Useful for testing with isolated storage
    pub fn new_with_logger(inner: Box<dyn CoreTool + Send + Sync>, logger: Arc<ToolCallLogger>) -> Self {
        Self { inner, logger }
    }

    /// Get the inner tool (unwrapped) for re-decorating
    pub fn inner(&self) -> &(dyn CoreTool + Send + Sync) {
        self.inner.as_ref()
    }

    /// Call the tool and return (result, entry)
    /// Used for manual call chain where entry is needed for upper layers
    pub async fn call_with_entry(
        &self,
        ctx: RequestContext,
        args: Value,
    ) -> (Result<Value>, ToolCallEntry) {
        let call_id = Uuid::now_v7().to_string();
        let started_at = current_timestamp_ms();
        let po = self.inner.po();

        // Execute the actual tool call
        let result = self.inner.call(ctx.clone(), args.clone()).await;
        let finished_at = current_timestamp_ms();
        let duration_ms = finished_at - started_at;

        let args_cloned = args.clone();

        // Parse result for logging
        let output_json: Option<Value> = match &result {
            Ok(v) => Some(v.clone()),
            Err(_) => None,
        };
        let (log_input, log_output, log_error) = redact_trace_values_for_tool(
            po,
            args,
            output_json,
            result.as_ref().err().map(|e| e.to_string()),
        );

        // Build the log entry
        let entry = ToolCallEntry {
            call_id,
            tool_id: po.id.clone(),
            tool_name: po.name.clone(),
            agent_id: ctx.agent_id().cloned(),
            task_id: ctx.task_id().cloned(),
            project_id: ctx.project_id().cloned(),
            started_at: started_at.try_into().unwrap(),
            finished_at: finished_at.try_into().unwrap(),
            duration_ms: duration_ms.try_into().unwrap(),
            input: log_input,
            output: log_output,
            error: log_error,
            status: match &result {
                Ok(_) => ToolCallStatus::Completed,
                Err(_) => ToolCallStatus::Failed,
            },
            metadata: Value::Null,
        };

        // Write the log entry - ignore logging errors, don't fail the actual call
        let _ = self.logger.log_call(&po.id, entry.clone());

        // Record tool call stat event for metrics aggregation
        // This covers ALL tool calls (manual + auto), ensuring complete stats coverage
        let _ = record_tool_call_stat(ctx.clone(), &entry, &args_cloned);

        (result, entry)
    }
}

fn record_tool_call_stat(ctx: RequestContext, entry: &ToolCallEntry, args: &Value) {
    let args_len = serde_json::to_string(args)
        .map(|s| s.len() as u64)
        .unwrap_or(0);
    let result_len = entry.output
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok())
        .map(|s| s.len() as u64)
        .unwrap_or(0);

    let status = if matches!(entry.status, ToolCallStatus::Completed) {
        "success".to_string()
    } else {
        "failed".to_string()
    };

    let timestamp = entry.finished_at as i64;
    let event = ToolCallEvent::new(timestamp)
        .with_tool_id(entry.tool_id.clone())
        .with_tool_name(entry.tool_name.clone())
        .with_agent_id(entry.agent_id.clone())
        .with_project_id(entry.project_id.clone())
        .with_task_id(entry.task_id.clone())
        .with_organization_id(ctx.organization_id().cloned())
        .with_user_id(ctx.user_id().cloned())
        .with_args_len(args_len)
        .with_result_len(result_len)
        .with_duration_ms(entry.duration_ms)
        .with_status(status);

    if let Some(stats) = ctx.stats_opt() {
        let ctx_clone = ctx.clone();
        let _ = stats.record(ctx_clone, event);
    }
}

fn redact_trace_values_for_tool(
    po: &ToolPo,
    input: Value,
    output: Option<Value>,
    error: Option<String>,
) -> (Value, Option<Value>, Option<String>) {
    if !matches!(po.protocol, ToolProtocol::Http | ToolProtocol::Mcp) {
        return (input, output, error);
    }

    (
        Value::String("[REDACTED]".to_string()),
        output.map(|_| Value::String("[REDACTED]".to_string())),
        error.map(|_| "[REDACTED]".to_string()),
    )
}

#[async_trait]
impl CoreTool for LoggingDecorator {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // call_with_entry 内部已经执行 log_call，这里不再重复写入
        // 修复：之前 Rig 调用 Auto 工具时，call_with_entry 和 call 各写一次，
        // 产生两条相同 call_id 的 trace 记录，污染统计和查询
        let (result, _entry) = self.call_with_entry(ctx, args).await;
        result
    }

    fn po(&self) -> &ToolPo {
        self.inner.po()
    }

    fn as_original(&self) -> &(dyn CoreTool + Send + Sync) {
        // inner is already the original - if it's decorated, inner would handle it
        // but since inner is already a dyn object, we can't call as_original on it (Sized requirement)
        // so just return the inner reference directly
        self.inner.as_ref()
    }
}
