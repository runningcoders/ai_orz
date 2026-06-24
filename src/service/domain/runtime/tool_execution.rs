//! Runtime Tool Execution 具体实现

use crate::error::AppError;
use crate::models::tool::{Tool, ToolExecutionError, ToolExecutionResult};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::pkg::tool_tracing::logger::{ToolCallLogger, ToolCallQuery};
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use rig::tool::ToolError;
use serde_json::Value;

#[async_trait::async_trait]
impl RuntimeToolExecution for RuntimeDomainImpl {
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult, ToolExecutionError> {
        let tool = self
            .tool_dal
            .get_by_id(ctx.clone(), tool_id.clone())
            .await
            .map_err(|e| {
                ToolExecutionError::without_trace(ToolError::ToolCallError(e.to_string().into()))
            })?
            .ok_or_else(|| {
                ToolExecutionError::without_trace(ToolError::ToolCallError(
                    format!("Tool not found: {}", tool_id).into(),
                ))
            })?;

        self.call_tool(ctx, &tool, args).await
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<ToolExecutionResult, ToolExecutionError> {
        let tool_id = tool.po.id.clone();
        ensure_tool_enabled(&tool_id, &tool.po.status)
            .map_err(ToolExecutionError::without_trace)?;

        let execution = match tool.po.protocol {
            ToolProtocol::Mcp => self.mcp_tool_dal.call_tool(ctx, tool, args).await,
            ToolProtocol::Builtin | ToolProtocol::Http => {
                self.tool_dal.call_tool(ctx, tool, args).await
            }
        };

        let (result, entry) = match execution {
            Ok((result, entry)) => (result, entry),
            Err(error) => {
                let mapped_error = match tool.po.protocol {
                    ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, error.error),
                    ToolProtocol::Builtin | ToolProtocol::Http => error.error,
                };
                return Err(ToolExecutionError {
                    error: mapped_error,
                    trace_ref: error.trace_ref,
                });
            }
        };

        Ok(ToolExecutionResult::new(
            result,
            entry.tool_id,
            entry.call_id,
        ))
    }

    async fn call_manual_tool_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: String,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult, ToolExecutionError> {
        let bound_tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), &agent_id)
            .await
            .map_err(|e| {
                ToolExecutionError::without_trace(ToolError::ToolCallError(e.to_string().into()))
            })?;

        let tool = bound_tools
            .iter()
            .find(|tool| tool.po.id == tool_id)
            .ok_or_else(|| {
                ToolExecutionError::without_trace(ToolError::ToolCallError(
                    format!(
                        "Manual tool call denied: tool {} is not bound to agent {}",
                        tool_id, agent_id
                    )
                    .into(),
                ))
            })?;

        if tool.po.control_mode != ControlMode::Manual {
            return Err(ToolExecutionError::without_trace(ToolError::ToolCallError(
                format!(
                    "Manual tool call denied: tool {} has control mode {:?}",
                    tool_id, tool.po.control_mode
                )
                .into(),
            )));
        }

        self.call_tool(ctx, tool, args).await
    }

    async fn query_tool_call_entries(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Vec<ToolCallEntry>, AppError> {
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        ToolCallLogger::get()
            .query_calls(query)
            .map_err(AppError::from)
    }

    async fn get_tool_call_entry_by_id(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Option<ToolCallEntry>, AppError> {
        super::tool_call_query::ensure_call_id_present(&query)?;
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        let mut entries = ToolCallLogger::get()
            .query_calls(query)
            .map_err(AppError::from)?;
        Ok(entries.pop())
    }
}

fn ensure_tool_enabled(tool_id: &str, status: &ToolStatus) -> Result<(), ToolError> {
    if *status != ToolStatus::Enabled {
        return Err(ToolError::ToolCallError(
            format!(
                "Tool execution denied: tool {} has status {:?}",
                tool_id, status
            )
            .into(),
        ));
    }

    Ok(())
}

fn map_mcp_tool_error(tool_id: &str, error: ToolError) -> ToolError {
    let message = error.to_string();
    let normalized = message.to_lowercase();
    let safe_message = if normalized.contains("timed out") || normalized.contains("timeout") {
        format!("MCP tool call timed out for tool_id: {}", tool_id)
    } else if normalized.contains("server") && normalized.contains("not found") {
        format!("MCP server not found for tool_id: {}", tool_id)
    } else if normalized.contains("server") && normalized.contains("disabled") {
        format!("MCP server disabled for tool_id: {}", tool_id)
    } else if normalized.contains("tool") && normalized.contains("disabled") {
        format!("MCP tool disabled: {}", tool_id)
    } else if normalized.contains("tool") && normalized.contains("not found") {
        format!("MCP tool not found: {}", tool_id)
    } else {
        format!("MCP tool call failed for tool_id: {}", tool_id)
    };

    ToolError::ToolCallError(safe_message.into())
}
