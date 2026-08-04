//! Default implementation of ToolCallDao

use crate::models::tool::{CoreTool, Tool, ToolCallTraceRef, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::get_registry;
use crate::pkg::tool_tracing::ToolCallLoggingDecorator;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use super::ToolCallDao;

// ==================== 工厂方法 + 单例 ====================

/// Global MCP-enhanced ToolCall DAO instance.
static MCP_TOOL_CALL_DAO: OnceLock<Arc<dyn super::mcp::McpToolCallDao + Send + Sync>> =
    OnceLock::new();

/// 创建一个全新的 ToolCall DAO 实例（用于测试）
pub fn new() -> Arc<dyn ToolCallDao> {
    Arc::new(ToolCallDaoImpl::new())
}

/// Get global ToolCall DAO
pub fn dao() -> Arc<dyn ToolCallDao> {
    mcp_dao()
}

/// Get global MCP-enhanced ToolCall DAO.
pub fn mcp_dao() -> Arc<dyn super::mcp::McpToolCallDao + Send + Sync> {
    MCP_TOOL_CALL_DAO.get().cloned().unwrap()
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
    MCP_TOOL_CALL_DAO.get_or_init(|| {
        Arc::new(super::mcp::McpToolCallDaoImpl::new(
            new(),
            Arc::new(crate::pkg::tool_registry::mcp::McpClientRuntime::default()),
        ))
    });
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

    fn decorate(&self, tool: Box<dyn CoreTool + Send + Sync>) -> Box<dyn CoreTool + Send + Sync> {
        Box::new(ToolCallLoggingDecorator::new(tool))
    }

    async fn call_manual(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
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

        match result {
            Ok(value) => {
                // 返回真实 entry（含 LoggingDecorator 生成的 call_id）
                // 调用方应使用 entry.call_id 构造 ToolExecutionResult，不再伪造
                Ok((value, entry))
            }
            Err(error) => {
                use common::error::{ErrorCode, ErrorType};
                let mut err = common::error::Error::typed(
                    ErrorCode::ToolExecutionFailed,
                    ErrorType::Tool,
                    error.to_string(),
                )
                .with_source(error);
                // 失败时 entry 被 consume 构造 trace_ref，Error 已携带 trace_ref
                let trace_ref = ToolCallTraceRef {
                    tool_id: entry.tool_id,
                    call_id: entry.call_id,
                };
                let mut field = common::error::ErrorField::new();
                field.set_trace_ref(trace_ref);
                err = err.with_field(field);
                Err(err.into())
            }
        }
    }
}
