//! Default implementation of ToolCallDao

use common::enums::tool::ControlMode;
use crate::models::tool::{Tool, ToolPo, CoreTool, RigToolAdapter};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::get_registry;
use crate::pkg::tool_tracing::{ToolCallLoggingDecorator};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use rig::tool::{ToolDyn, ToolError};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use super::ToolCallDao;

// ==================== 工厂方法 + 单例 ====================

/// Global ToolCall DAO instance
static TOOL_CALL_DAO: OnceLock<Arc<dyn ToolCallDao>> = OnceLock::new();

/// 创建一个全新的 ToolCall DAO 实例（用于测试）
pub fn new() -> Arc<dyn ToolCallDao> {
    Arc::new(ToolCallDaoImpl::new())
}

/// Get global ToolCall DAO
pub fn dao() -> Arc<dyn ToolCallDao> {
    TOOL_CALL_DAO.get().cloned().unwrap()
}

/// ToolCall DAO implementation
#[derive(Clone, Default)]
pub struct ToolCallDaoImpl {}

impl ToolCallDaoImpl {
    fn new() -> Self {
        Self {}
    }
}

/// Initialize global ToolCall DAO
pub fn init() {
    TOOL_CALL_DAO.set(new()).ok();
}

#[async_trait]
impl ToolCallDao for ToolCallDaoImpl {
    fn assemble_core_tool(&self, po: &ToolPo) -> Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        let registry = get_registry();
        let Some(tool_raw) = registry.create_tool(po.clone()) else {
            return Ok(None);
        };

        // Coerce to Box<dyn CoreTool + Send + Sync>
        let tool_raw: Box<dyn CoreTool + Send + Sync> = tool_raw;

        Ok(Some(tool_raw))
    }

    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
        let mut rig_tools = Vec::new();

        for tool in tools {
            // Only include tools that are Auto mode (automatic invocation by Rig)
            if tool.po.control_mode != ControlMode::Auto {
                continue;
            }

            // Clone the core tool (we need our own copy for wrapping)
            let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);

            // Wrap with logging decorator to capture logs
            let decorated = ToolCallLoggingDecorator::new(cloned);
            let decorated_box: Box<dyn CoreTool + Send + Sync> = Box::new(decorated);

            // Adapt to Rig's ToolDyn interface
            let rig_adapter = RigToolAdapter::new(ctx.clone(), decorated_box);
            let rig_adapter_box: Box<dyn ToolDyn> = Box::new(rig_adapter);

            rig_tools.push(rig_adapter_box);
        }

        rig_tools
    }

    async fn call_manual(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry), ToolError> {
        // our_tool is always raw (not pre-decorated) - clone and create a new decorator for this call
        // this guarantees we get a fresh entry for this specific invocation
        let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);
        let decorated = ToolCallLoggingDecorator::new(cloned);

        // Call with entry capture
        let (result, mut entry) = decorated.call_with_entry(ctx, args).await;

        // Add caller location for debugging
        let location = std::panic::Location::caller();
        let location_str = format!("{}:{}", location.file(), location.line());
        if let serde_json::Value::Object(ref mut map) = entry.metadata {
            map.insert("caller_location".to_string(), Value::String(location_str));
        } else {
            let mut map = serde_json::Map::new();
            map.insert("caller_location".to_string(), Value::String(location_str));
            entry.metadata = Value::Object(map);
        }

        result.map(|v| (v, entry))
    }
}
