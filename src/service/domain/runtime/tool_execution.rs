//! Runtime Tool Execution 具体实现

use common::error::{bail_err, Error, Result};
use crate::models::tool::{Tool, ToolExecutionResult};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::pkg::tool_tracing::logger::ToolCallQuery;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use serde_json::Value;

#[async_trait::async_trait]
impl RuntimeToolExecution for RuntimeDomainImpl {
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult> {
    let tool = self
        .tool_dal
        .get_by_id(ctx.clone(), tool_id.clone())
        .await?
        .ok_or_else(|| {
            common::error::Error::tool_call_failed(format!("Tool not found: {}", tool_id))
        })?;

        self.call_tool(ctx, &tool, args).await
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<ToolExecutionResult> {
        let tool_id = tool.po.id.clone();
        ensure_tool_enabled(&tool_id, &tool.po.status)?;

        let execution = match tool.po.protocol {
            ToolProtocol::Mcp => self.mcp_tool_dal.call_tool(ctx, tool, args).await,
            ToolProtocol::Builtin | ToolProtocol::Http => {
                self.tool_dal.call_tool(ctx, tool, args).await
            }
        };

        let (result, entry) = match execution {
            Ok((value, entry)) => (value, entry),
            Err(error) => {
                // 修复：保留原 error 的 field（含 trace_ref），不再构造新 Error 丢弃 field
                let mapped_message: String = match tool.po.protocol {
                    ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
                    ToolProtocol::Builtin | ToolProtocol::Http => error.to_string(),
                };
                let mut new_err = common::error::Error::new(
                    common::error::ErrorCode::ToolExecutionFailed,
                    mapped_message,
                );
                if let Some(field) = error.field() {
                    new_err = new_err.with_field(field.clone());
                }
                new_err = new_err.with_source(error);
                return Err(new_err);
            }
        };

        // 修复：使用 LoggingDecorator 生成的真实 call_id（entry.call_id），不再伪造
        Ok(ToolExecutionResult::new(
            result,
            entry.tool_id.clone(),
            entry.call_id.clone(),
        ))
    }

    async fn call_manual_tool_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: String,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult> {
        let ctx = ctx.to_builder().agent_id(&agent_id).build();

        // 先从绑定工具中查找
        let bound_tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), &agent_id)
            .await?;

        let bound_tool = bound_tools
            .into_iter()
            .find(|tool| tool.po.id == tool_id);

        // 如果绑定工具中没有，检查是否是神经工具或已安装工具包
        let tool = match bound_tool {
            Some(tool) => tool,
            None => {
                // 获取 agent 的 installed_tags
                let installed_tags = self
                    .agent_dal
                    .find_by_id(ctx.clone(), &agent_id)
                    .await?
                    .map(|agent| agent.po.get_installed_tags())
                    .unwrap_or_default();

                // 构建 tag 过滤列表：neural + installed_tags（OR 语义）
                // SQL 层直接过滤，避免全量加载工具到内存
                let mut tag_filter = vec!["neural".to_string()];
                tag_filter.extend(installed_tags.clone());

                let candidate_tools = self
                    .tool_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::tool::ToolQuery {
                            tags: Some(tag_filter),
                            enabled_only: Some(true),
                            ..Default::default()
                        },
                    )
                    .await?;

                // 在 SQL 过滤后的候选工具中按 ID 精确匹配
                candidate_tools
                    .into_iter()
                    .find(|t| t.po.id == tool_id)
                    .ok_or_else(|| {
                        if installed_tags.is_empty() {
                            common::error::Error::tool_call_failed(format!(
                                "Manual tool call denied: tool {} is not bound to agent {}, not a neural tool, and agent has no installed tool packs",
                                tool_id, agent_id
                            ))
                        } else {
                            common::error::Error::tool_call_failed(format!(
                                "Manual tool call denied: tool {} is not bound to agent {}, not a neural tool, and does not belong to any installed tool pack (installed: {:?})",
                                tool_id, agent_id, installed_tags
                            ))
                        }
                    })?
            }
        };

        if tool.po.control_mode != ControlMode::Manual {
            let msg: String = format!(
                "Manual tool call denied: tool {} has control mode {:?}",
                tool_id, tool.po.control_mode
            );
            let msg: String = msg;
            return Err(common::error::Error::new(
                common::error::ErrorCode::ToolExecutionFailed,
                msg
            ));

        }

        self.call_tool(ctx, &tool, args).await
    }

    async fn query_tool_call_entries(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Vec<ToolCallEntry>> {
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        Ok(self.tool_call_logger
            .query_calls(query)?)
    }

    async fn get_tool_call_entry_by_id(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Option<ToolCallEntry>> {
        super::tool_call_query::ensure_call_id_present(&query)?;
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        let mut entries = self.tool_call_logger
            .query_calls(query)?;
        Ok(entries.pop())
    }
}

fn ensure_tool_enabled(tool_id: &str, status: &ToolStatus) -> Result<()> {
    if *status != ToolStatus::Enabled {
        bail_err!(ToolExecutionFailed, "Tool execution denied: tool {} has status {:?}", tool_id, status);
    }

    Ok(())
}

fn map_mcp_tool_error(tool_id: &str, error: &Error) -> String {
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

    safe_message
}
