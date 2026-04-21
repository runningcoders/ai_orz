//! Logging decorator for Rig's ToolDyn
//!
//! Wraps built-in tools that are called directly by Rig to automatically log invocations.

use anyhow::Result;
use rig::tool::ToolDyn;
use serde_json::{Value, from_str};
use uuid::Uuid;
use std::pin::Pin;

use crate::pkg::request_context::RequestContext;
use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;
use common::constants::utils::current_timestamp_ms;

/// Logging decorator that wraps a ToolDyn instance and automatically logs all calls
pub struct LoggingToolDecorator {
    /// The inner tool that actually does the work
    inner: Box<dyn ToolDyn + Send + Sync>,
    /// Tool PO metadata
    tool_id: String,
    /// Tool name
    tool_name: String,
    /// Full request context for this invocation
    /// Extracted at construction time because Rig calls do not provide context
    ctx: RequestContext,
    /// Logger instance
    logger: ToolCallLogger,
}

impl LoggingToolDecorator {
    /// Create a new logging decorator wrapping an existing tool
    pub fn new(
        inner: Box<dyn ToolDyn + Send + Sync>,
        tool_id: String,
        tool_name: String,
        ctx: &RequestContext,
        logger: ToolCallLogger,
    ) -> Self {
        Self {
            inner,
            tool_id,
            tool_name,
            ctx: ctx.clone(),
            logger,
        }
    }
}

impl ToolDyn for LoggingToolDecorator {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn definition(
        &self,
        prefix: String,
    ) -> Pin<Box<dyn futures_util::Future<Output = rig::completion::ToolDefinition> + Send + '_>> {
        self.inner.definition(prefix)
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<
        Box<
            dyn futures_util::Future<
                    Output = Result<String, rig::tool::ToolError>
                >
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let call_id = Uuid::now_v7().to_string();
            let started_at = current_timestamp_ms();

            // Parse JSON args from string
            let args_json: Value = match from_str(&args) {
                Ok(v) => v,
                Err(e) => {
                    return Err(e.into());
                }
            };

            // Execute the actual tool call - inner is already correct type
            let result = self.inner.call(args).await;
            let finished_at = current_timestamp_ms();
            let duration_ms = finished_at - started_at;

            // Parse result json for logging
            let output_json: Option<Value> = match &result {
                Ok(output_str) => serde_json::from_str(output_str).ok(),
                Err(_) => None,
            };

            // Build the log entry - write only once (after completion) per the spec
            let entry = ToolCallEntry {
                call_id,
                tool_id: self.tool_id.clone(),
                tool_name: self.tool_name.clone(),
                agent_id: self.ctx.agent_id().cloned(),
                task_id: self.ctx.task_id().cloned(),
                project_id: self.ctx.project_id().cloned(),
                started_at: started_at.try_into().unwrap(),
                finished_at: finished_at.try_into().unwrap(),
                duration_ms: duration_ms.try_into().unwrap(),
                input: args_json,
                output: output_json,
                error: result.as_ref().err().map(|e| e.to_string()),
                status: match &result {
                    Ok(_) => ToolCallStatus::Completed,
                    Err(_) => ToolCallStatus::Failed,
                },
                metadata: Value::Null,
            };

            // Write the log entry - ignore logging errors, don't fail the actual call
            // because logging is optional/observability
            let _ = self.logger.log_call(&self.tool_id, entry);

            result
        })
    }
}
