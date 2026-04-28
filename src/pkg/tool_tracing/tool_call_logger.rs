//! Logging decorator for our core Tool trait (with explicit RequestContext)
//!
//! Wraps tools that are called through our manual built-in call chain
//! to automatically log invocations the same way.

use anyhow::Result;
use serde_json::{Value};
use uuid::Uuid;
use async_trait::async_trait;
use rig::tool::ToolError;

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;
use common::constants::utils::current_timestamp_ms;

/// Logging decorator that wraps a Tool instance and automatically logs all calls
#[derive(Clone)]
pub struct LoggingDecorator {
    /// The inner tool that actually does the work
    inner: Box<dyn CoreTool + Send + Sync>,
}

impl LoggingDecorator {
    /// Create a new logging decorator wrapping an existing tool
    pub fn new(
        inner: Box<dyn CoreTool + Send + Sync>,
    ) -> Self {
        Self { inner }
    }

    /// Get the inner tool (unwrapped) for re-decorating
    pub fn inner(&self) -> &(dyn CoreTool + Send + Sync) {
        self.inner.as_ref()
    }

    /// Call the tool and return (result, entry)
    /// Used for manual call chain where entry is needed for upper layers
    pub async fn call_with_entry(
        &self,
        ctx: &RequestContext,
        args: Value,
    ) -> (Result<Value, ToolError>, ToolCallEntry) {
        let call_id = Uuid::now_v7().to_string();
        let started_at = current_timestamp_ms();
        let po = self.inner.po();

        // Execute the actual tool call
        let result = self.inner.call(ctx, args.clone()).await;
        let finished_at = current_timestamp_ms();
        let duration_ms = finished_at - started_at;

        // Parse result for logging
        let output_json: Option<Value> = match &result {
            Ok(v) => Some(v.clone()),
            Err(_) => None,
        };

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
            input: args,
            output: output_json,
            error: result.as_ref().err().map(|e| e.to_string()),
            status: match &result {
                Ok(_) => ToolCallStatus::Completed,
                Err(_) => ToolCallStatus::Failed,
            },
            metadata: Value::Null,
        };

        // Write the log entry - ignore logging errors, don't fail the actual call
        let _ = ToolCallLogger::get().log_call(&po.id, entry.clone());

        (result, entry)
    }
}

#[async_trait]
impl CoreTool for LoggingDecorator {
    async fn call(&self, ctx: &RequestContext, args: Value) -> Result<Value, ToolError> {
        let (result, entry) = self.call_with_entry(ctx, args).await;
        // Log the entry immediately
        let tool_id = entry.tool_id.clone();
        ToolCallLogger::get().log_call(&tool_id, entry);
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