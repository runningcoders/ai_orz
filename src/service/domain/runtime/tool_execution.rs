//! Runtime Tool Execution 具体实现

use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::enums::ToolProtocol;
use rig::tool::ToolError;
use serde_json::Value;

#[async_trait::async_trait]
impl RuntimeToolExecution for RuntimeDomainImpl {
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<Value, ToolError> {
        let tool = self
            .tool_dal
            .get_by_id(ctx.clone(), tool_id.clone())
            .await
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
            .ok_or_else(|| {
                ToolError::ToolCallError(format!("Tool not found: {}", tool_id).into())
            })?;

        match tool.po.protocol {
            ToolProtocol::Mcp => self
                .mcp_tool_dal
                .call_tool_by_id(ctx, tool_id.clone(), args)
                .await
                .map_err(|_| {
                    ToolError::ToolCallError(
                        format!("MCP tool call failed for tool_id: {}", tool_id).into(),
                    )
                }),
            ToolProtocol::Builtin | ToolProtocol::Http => {
                self.tool_dal.call_tool_by_id(ctx, tool_id, args).await
            }
        }
    }
}
