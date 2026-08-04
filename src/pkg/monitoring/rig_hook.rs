//! Runtime Monitoring - 运行时监控框架
//!
//! 负责对 LLM 推理运行时进行监控，支持：
//! - Token 使用统计（自动写入 Stats）
//! - 调用日志记录
//! - 耗时监控
//! - 工具调用审计

use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::ModelCallEvent;
use rig::agent::{
    AgentHook, CompletionCallAction, CompletionCallEvent, CompletionResponseEvent, HookContext,
    ObservationAction, ToolCall, ToolCallAction, ToolResultAction, ToolResultEvent,
};
use tracing::{debug, info, warn};

/// Runtime Monitoring Hook
/// 接入 rig 0.41 的 AgentHook 机制，实现运行时监控
///
/// 在 `on_completion_response` 中自动将 token 用量写入 Stats，
/// tags 从 `RequestContext` 提取（agent_id / task_id / project_id /
/// model_provider_id / model_name / organization_id / user_id），
/// metrics 包含 tokens_input / tokens_output / total_tokens。
#[derive(Clone)]
pub struct RuntimeMonitoringHook {
    ctx: RequestContext,
}

impl RuntimeMonitoringHook {
    pub fn new(ctx: RequestContext) -> Self {
        Self { ctx }
    }
}

impl AgentHook for RuntimeMonitoringHook {
    /// Called before the prompt is sent to the model
    fn on_completion_call(
        &self,
        _ctx: &HookContext,
        event: CompletionCallEvent<'_>,
    ) -> impl futures_util::Future<Output = CompletionCallAction> + rig::wasm_compat::WasmCompatSend
    {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            history_len = event.history.len(),
            "Starting completion call"
        );
        async { CompletionCallAction::continue_run() }
    }

    /// Called after the prompt is sent to the model and a response is received.
    ///
    /// 自动记录 token 用量到 Stats，便于后续按 agent / project / task /
    /// model_provider 等维度聚合统计。
    fn on_completion_response(
        &self,
        _ctx: &HookContext,
        event: CompletionResponseEvent<'_>,
    ) -> impl futures_util::Future<Output = ObservationAction> + rig::wasm_compat::WasmCompatSend
    {
        let usage = event.usage;
        info!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            organization_id = ?self.ctx.organization_id,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            total_tokens = usage.total_tokens,
            "Completion response received - Token usage recorded"
        );

        let ctx = self.ctx.clone();
        let agent_id = self.ctx.agent_id().cloned();
        let project_id = self.ctx.project_id().cloned();
        let task_id = self.ctx.task_id().cloned();
        let model_provider_id = self.ctx.model_provider_id().cloned();
        let model_name = self.ctx.model_name().cloned();
        let organization_id = self.ctx.organization_id().cloned();
        let user_id = self.ctx.user_id().cloned();

        async move {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let event = ModelCallEvent::new(timestamp)
                .with_agent_id(agent_id)
                .with_project_id(project_id)
                .with_task_id(task_id)
                .with_model_provider_id(model_provider_id)
                .with_model_name(model_name)
                .with_organization_id(organization_id)
                .with_user_id(user_id)
                .with_tokens_input(usage.input_tokens)
                .with_tokens_output(usage.output_tokens)
                .with_total_tokens(usage.total_tokens);

            if let Err(e) = ctx.stats().record(ctx.clone(), event).await {
                warn!(
                    log_id = ctx.log_id,
                    error = %e,
                    "Failed to record stats event for completion response"
                );
            }
            ObservationAction::continue_run()
        }
    }

    /// Called before a tool call is executed.
    fn on_tool_call(
        &self,
        _ctx: &HookContext,
        event: ToolCall<'_>,
    ) -> impl futures_util::Future<Output = ToolCallAction> + rig::wasm_compat::WasmCompatSend {
        debug!(
            log_id = self.ctx.log_id,
            user_id = ?self.ctx.user_id,
            tool_name = event.tool_name,
            tool_call_id = ?event.tool_call_id,
            internal_call_id = %event.internal_call_id,
            args_length = event.args.len(),
            "Tool call starting"
        );
        async { ToolCallAction::run() }
    }

    /// Called after a tool call has been executed.
    ///
    /// 工具调用统计已在 ToolCallLoggingDecorator 中统一记录，
    /// 此处仅保留日志记录用于调试。
    fn on_tool_result(
        &self,
        _ctx: &HookContext,
        event: ToolResultEvent<'_>,
    ) -> impl futures_util::Future<Output = ToolResultAction> + rig::wasm_compat::WasmCompatSend
    {
        debug!(
            log_id = self.ctx.log_id,
            tool_name = event.tool_name,
            tool_call_id = ?event.tool_call_id,
            internal_call_id = %event.internal_call_id,
            args_length = event.args.len(),
            "Tool call completed"
        );

        async { ToolResultAction::keep() }
    }
}
