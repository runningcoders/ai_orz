//! Default implementation of ToolCallDao

use crate::models::events::ToolExecEvent;
use crate::models::tool::{CoreTool, Tool, ToolCallTraceRef, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::get_registry;
use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
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

    async fn execute(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry)> {
        // 直接调用工具并内联构造 trace entry，发布 AOP ToolExecEvent
        // （取代被移除的 ToolCallLoggingDecorator）
        //
        // call_id 单一事实源：业务指定（ctx 已携带 tool_call_id）优先复用，
        // 未指定时此处单点生成新 UUID v7，并注入 ctx 供工具内部关联使用。
        let business_specified = ctx.tool_call_id().is_some();
        let call_id = ctx
            .tool_call_id()
            .cloned()
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

        let po = tool.our_tool.po();

        // 幂等防重：仅业务指定 call_id 时查询历史（自动生成新 UUID 永不命中，避免多余扫描）
        // 历史 Completed → 直接返回历史结果；Failed → 允许重试正常执行
        if business_specified
            && let Ok(Some(history)) = crate::pkg::tool_tracing::logger::ToolCallLogger::get()
                .read_call_by_id(Some(po.id.as_str()), &call_id)
            && history.status == ToolCallStatus::Completed
        {
            let mut entry = history;
            if let Value::Object(map) = &mut entry.metadata {
                map.insert("deduplicated".to_string(), Value::Bool(true));
            }
            let output = entry.output.clone().unwrap_or(Value::Null);
            return Ok((output, entry));
        }

        // 注入 call_id 后再调用工具，工具内部可通过 ctx.tool_call_id() 关联日志文件/进程条目
        let ctx = ctx.to_builder().tool_call_id(call_id.clone()).build();
        let started_at = common::constants::utils::current_timestamp_ms() as u64;

        let cloned: Box<dyn CoreTool + Send + Sync> = dyn_clone::clone_box(&*tool.our_tool);
        let result = cloned.call(ctx.clone(), args.clone()).await;

        let finished_at = common::constants::utils::current_timestamp_ms() as u64;
        let duration_ms = finished_at.saturating_sub(started_at);

        // 计算 args 长度（序列化为 JSON 字符串后的字节数）
        let args_len = serde_json::to_string(&args)
            .map(|s| s.len() as u64)
            .unwrap_or(0);

        // 根据成功/失败构造 output / error，并计算 result_len
        let (output_json, error_str, result_len, status) = match &result {
            Ok(value) => {
                let len = serde_json::to_string(value)
                    .map(|s| s.len() as u64)
                    .unwrap_or(0);
                (Some(value.clone()), None, len, ToolCallStatus::Completed)
            }
            Err(error) => {
                let err_msg = error.to_string();
                let len = err_msg.len() as u64;
                (None, Some(err_msg), len, ToolCallStatus::Failed)
            }
        };

        // 对外部协议工具（HTTP/MCP）的 input/output/error 进行脱敏
        let (input_redacted, output_redacted, error_redacted) =
            crate::pkg::tool_tracing::entry::redact_trace_values_for_tool(
                po,
                args,
                output_json,
                error_str,
            );

        let mut entry = ToolCallEntry {
            call_id,
            tool_id: po.id.clone(),
            tool_name: po.name.clone(),
            agent_id: ctx.agent_id().cloned(),
            task_id: ctx.task_id().cloned(),
            project_id: ctx.project_id().cloned(),
            started_at,
            finished_at,
            duration_ms,
            input: input_redacted,
            output: output_redacted,
            error: error_redacted,
            status,
            metadata: Value::Object(serde_json::Map::new()),
        };

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

        // 发布 AOP ToolExecEvent（同步消费：日志写入 + 统计记录）
        let event = ToolExecEvent::new(
            entry.clone(),
            ctx.organization_id().cloned(),
            ctx.user_id().cloned(),
            args_len,
            result_len,
        );
        crate::pkg::aop::publish(event).await;

        match result {
            Ok(value) => Ok((value, entry)),
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
                    tool_id: entry.tool_id.clone(),
                    call_id: entry.call_id.clone(),
                };
                let mut field = common::error::ErrorField::new();
                field.set_trace_ref(trace_ref);
                err = err.with_field(field);
                Err(err.into())
            }
        }
    }
}
