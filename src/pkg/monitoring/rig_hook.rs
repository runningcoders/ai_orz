//! Runtime Monitoring - 运行时监控框架
//!
//! 负责对 LLM 推理运行时进行监控，支持：
//! - Token 使用统计
//! - 调用日志记录
//! - 耗时监控
//! - 工具调用审计

use crate::pkg::request_context::RequestContext;
use rig::agent::{HookAction, PromptHook, ToolCallHookAction};
use rig::completion::{CompletionModel, CompletionResponse, Message};
use rig::wasm_compat::WasmCompatSend;
use tracing::{debug, info};
use common::bail_err;

/// Runtime Monitoring Hook
/// 接入 rig 的 hook 机制，实现运行时监控
#[derive(Clone)]
pub struct RuntimeMonitoringHook {
    ctx: RequestContext,
}

impl RuntimeMonitoringHook {
    pub fn new(ctx: RequestContext) -> Self {
        Self { ctx }
    }
}

impl<M> PromptHook<M> for RuntimeMonitoringHook
where
    M: CompletionModel,
{
    /// Called before the prompt is sent to the model
    fn on_completion_call(
        &self,
        _prompt: &Message,
        history: &[Message],
    ) -> impl futures_util::Future<Output = HookAction> + WasmCompatSend {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            history_len = history.len(),
            "Starting completion call"
        );
        Box::pin(async { HookAction::cont() })
    }

    /// Called after the prompt is sent to the model and a response is received.
    fn on_completion_response(
        &self,
        _prompt: &Message,
        response: &CompletionResponse<M::Response>,
    ) -> impl futures_util::Future<Output = HookAction> + WasmCompatSend {
        let usage = response.usage;
        info!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            total_tokens = usage.total_tokens,
            "Completion response received - Token usage recorded"
        );
        Box::pin(async { HookAction::cont() })
    }

    /// Called before a tool call is executed.
    fn on_tool_call(
        &self,
        tool_name: &str,
        tool_call_id: Option<String>,
        internal_call_id: &str,
        args: &str,
    ) -> impl futures_util::Future<Output = ToolCallHookAction> + WasmCompatSend {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            tool_name = tool_name,
            tool_call_id = ?tool_call_id,
            internal_call_id = %internal_call_id,
            args_length = args.len(),
            "Tool call starting"
        );
        Box::pin(async { ToolCallHookAction::cont() })
    }

    /// Called after a tool call has been executed.
    fn on_tool_result(
        &self,
        tool_name: &str,
        tool_call_id: Option<String>,
        internal_call_id: &str,
        args: &str,
        result: &str,
    ) -> impl futures_util::Future<Output = HookAction> + WasmCompatSend {
        debug!(
            log_id = self.ctx.log_id,
            tool_name = tool_name,
            tool_call_id = ?tool_call_id,
            internal_call_id = %internal_call_id,
            args_length = args.len(),
            result_length = result.len(),
            "Tool call completed"
        );
        Box::pin(async { HookAction::cont() })
    }
}
