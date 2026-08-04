//! ToolCall DAO trait
//! Responsible for:
//! 1. Get CoreTool instance from registry by ToolPo metadata
//! 2. Wrap Tool's CoreTool into Rig's DynamicTool for Rig to use
//! 3. Manual call a Tool with logging decorator, returns (result, entry)

use crate::models::tool::{CoreTool, Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use anyhow::Result;
use async_trait::async_trait;

use std::boxed::Box;

pub mod r#impl;
pub mod mcp;

pub use r#impl::{dao, init, mcp_dao, new};
pub use mcp::{McpToolCallDao, new as new_mcp_tool_call_dao};

#[cfg(test)]
mod mcp_test;

/// ToolCall DAO trait
#[async_trait]
pub trait ToolCallDao: Send + Sync {
    /// Assemble CoreTool instance from ToolPo metadata
    /// Uses registry to create CoreTool instance based on PO's name/version
    fn assemble_core_tool(&self, po: &ToolPo) -> Result<Option<Box<dyn CoreTool + Send + Sync>>>;

    /// Wrap a list of Tools into Rig's DynamicTool objects
    /// Each CoreTool is wrapped with logging decorator then adapted for Rig
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<rig::tool::DynamicTool>;

    /// Call a tool manually (our controlled mode)
    /// Creates new logging decorator for this call, captures trace entry.
    ///
    /// 成功时返回 (Value, ToolCallEntry)，entry.call_id 为 LoggingDecorator
    /// 生成的真实 call_id，调用方应使用此 call_id 而非现场伪造。
    /// 失败时返回的 Error 已携带 trace_ref（从 entry 中提取）。
    async fn call_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: serde_json::Value,
    ) -> Result<(serde_json::Value, ToolCallEntry)>;
}
