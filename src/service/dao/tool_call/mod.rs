//! ToolCall DAO trait
//! Responsible for:
//! 1. Get CoreTool instance from registry by ToolPo metadata
//! 2. Wrap Tool's CoreTool into Rig's ToolDyn for Rig to use
//! 3. Manual call a Tool with logging decorator, returns (result, entry)

use crate::models::tool::{Tool, ToolPo, CoreTool};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use rig::tool::ToolError;
use anyhow::Result;
use async_trait::async_trait;
use std::boxed::Box;

pub mod r#impl;

pub use r#impl::{init, dao, new};

/// ToolCall DAO trait
#[async_trait]
pub trait ToolCallDao: Send + Sync {
    /// Assemble CoreTool instance from ToolPo metadata
    /// Uses registry to create CoreTool instance based on PO's name/version
    fn assemble_core_tool(&self, po: &ToolPo) -> Result<Option<Box<dyn CoreTool + Send + Sync>>>;

    /// Wrap a list of Tools into Rig's ToolDyn objects
    /// Each CoreTool is wrapped with logging decorator then adapted for Rig
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn rig::tool::ToolDyn>>;

    /// Call a tool manually (our controlled mode)
    /// Creates new logging decorator for this call, captures trace entry
    async fn call_manual(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: serde_json::Value,
    ) -> Result<(serde_json::Value, ToolCallEntry), ToolError>;
}
