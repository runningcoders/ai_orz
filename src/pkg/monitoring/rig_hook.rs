//! Runtime Monitoring - 运行时监控框架
//!
//! 负责对 LLM 推理运行时进行监控，支持：
//! - Token 使用统计（自动写入 Stats）
//! - 调用日志记录
//! - 耗时监控
//! - 工具调用审计

use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::{ModelCallEvent, ToolCallEvent};
use rig::agent::{HookAction, PromptHook, ToolCallHookAction};
use rig::completion::{CompletionModel, CompletionResponse, Message};
use rig::wasm_compat::WasmCompatSend;
use serde_json::{json, Map};
use tracing::{debug, info, warn};

/// Runtime Monitoring Hook
/// 接入 rig 的 hook 机制，实现运行时监控
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

    /// 从 RequestContext 构建 stats tags（仅包含有值的字段）
    fn build_tags(&self) -> Map<String, serde_json::Value> {
        let mut tags = Map::new();
        let ctx = &self.ctx;
        if let Some(v) = ctx.agent_id.as_ref() {
            tags.insert("agent_id".into(), json!(v));
        }
        if let Some(v) = ctx.task_id.as_ref() {
            tags.insert("task_id".into(), json!(v));
        }
        if let Some(v) = ctx.project_id.as_ref() {
            tags.insert("project_id".into(), json!(v));
        }
        if let Some(v) = ctx.model_provider_id.as_ref() {
            tags.insert("model_provider_id".into(), json!(v));
        }
        if let Some(v) = ctx.model_name.as_ref() {
            tags.insert("model_name".into(), json!(v));
        }
        if let Some(v) = ctx.organization_id.as_ref() {
            tags.insert("organization_id".into(), json!(v));
        }
        if let Some(v) = ctx.user_id.as_ref() {
            tags.insert("user_id".into(), json!(v));
        }
        tags
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
    ///
    /// 自动记录 token 用量到 Stats，便于后续按 agent / project / task /
    /// model_provider 等维度聚合统计。
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

        let ctx = self.ctx.clone();
        let tags = self.build_tags();
        let mut tags_map = tags;
        tags_map.insert("event_type".into(), json!("completion"));
        let metrics = json!({
            "call_count": 1,
            "tokens_input": usage.input_tokens,
            "tokens_output": usage.output_tokens,
            "total_tokens": usage.total_tokens,
        });

        Box::pin(async move {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let event = ModelCallEvent::new(timestamp)
                .with_tags(serde_json::Value::Object(tags_map))
                .with_metrics(metrics);

            if let Err(e) = ctx.stats().record(ctx.clone(), event).await {
                warn!(
                    log_id = ctx.log_id,
                    error = %e,
                    "Failed to record stats event for completion response"
                );
            }
            HookAction::cont()
        })
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
    ///
    /// 自动记录工具调用到 Stats，便于后续统计工具调用次数、QPS 等。
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

        let ctx = self.ctx.clone();
        let tool_name = tool_name.to_string();
        let args_len = args.len() as u64;
        let result_len = result.len() as u64;
        let mut tags = self.build_tags();
        tags.insert("event_type".into(), json!("tool_call"));
        tags.insert("tool_name".into(), json!(tool_name));

        let metrics = json!({
            "call_count": 1,
            "args_len": args_len,
            "result_len": result_len,
        });

        Box::pin(async move {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let event = ToolCallEvent::new(timestamp)
                .with_tags(serde_json::Value::Object(tags))
                .with_metrics(metrics);

            if let Err(e) = ctx.stats().record(ctx.clone(), event).await {
                warn!(
                    log_id = ctx.log_id,
                    error = %e,
                    "Failed to record stats event for tool call"
                );
            }
            HookAction::cont()
        })
    }
}
